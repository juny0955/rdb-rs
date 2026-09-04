use std::cmp::Ordering;

use crate::{
    index::btree::{
        BTreeKey,
        internal::{
            InternalEntry, InternalPageError, append_internal_entry, initialize_internal_page,
            internal_split, replace_child_after_leaf_split,
        },
        leaf::{LeafEntry, LeafPageError, append_leaf_entry, leaf_split},
        tree::{BTree, BTreeError},
    },
    storage::{
        buffer::{BufferPool, page_key::PageKey},
        page::{Page, PageId, RowId},
    },
};

impl BTree {
    pub fn insert(
        &self,
        buffer_pool: &mut BufferPool,
        key: BTreeKey,
        row_id: RowId,
    ) -> Result<(), BTreeError> {
        if !self.accepts_key(&key) {
            return Err(BTreeError::InvalidKeyType);
        }

        let page_id = self.find_leaf_page_id(buffer_pool, &key)?;
        let page_key = PageKey::new(self.relation_id, page_id);
        let new_entry = LeafEntry::new(key, row_id);
        let (root_bytes, is_root) = {
            let mut guard = buffer_pool.fetch_page(page_key)?;
            let page = guard.page_mut();
            match append_leaf_entry(page, &new_entry) {
                Ok(()) => (None, false),
                Err(LeafPageError::PageFull) => (
                    Some(guard.page().as_bytes().to_vec()),
                    page_id == self.root_page_id,
                ),
                Err(e) => return Err(e.into()),
            }
        };

        match root_bytes {
            None => buffer_pool.flush_page(page_key)?,
            Some(root_bytes) => {
                if is_root {
                    self.root_leaf_split(buffer_pool, root_bytes, new_entry)?;
                } else {
                    self.child_leaf_split(buffer_pool, root_bytes, page_id, new_entry)?;
                }
            }
        }

        Ok(())
    }

    fn root_leaf_split(
        &self,
        buffer_pool: &mut BufferPool,
        root_bytes: Vec<u8>,
        new_entry: LeafEntry,
    ) -> Result<(), BTreeError> {
        let left_page_id = buffer_pool.allocate_page(self.relation_id)?;
        let right_page_id = buffer_pool.allocate_page(self.relation_id)?;

        let mut left_page = Page::new_raw();
        let left_page_key = PageKey::new(self.relation_id, left_page_id);
        left_page.as_bytes_mut().copy_from_slice(&root_bytes);

        let mut right_page = Page::new_raw();
        let right_page_key = PageKey::new(self.relation_id, right_page_id);

        let separator = leaf_split(
            &mut left_page,
            &mut right_page,
            right_page_id,
            self.key_type,
            new_entry,
        )?;
        Self::fetch_and_flush_page(buffer_pool, left_page_key, left_page.as_bytes())?;
        Self::fetch_and_flush_page(buffer_pool, right_page_key, right_page.as_bytes())?;
        let root_page = self.root_internal_split(left_page_id, right_page_id, separator)?;
        Self::fetch_and_flush_page(
            buffer_pool,
            PageKey::new(self.relation_id, self.root_page_id),
            root_page.as_bytes(),
        )?;

        Ok(())
    }

    fn child_leaf_split(
        &self,
        buffer_pool: &mut BufferPool,
        leaf_bytes: Vec<u8>,
        old_child: PageId,
        new_entry: LeafEntry,
    ) -> Result<(), BTreeError> {
        let mut parent = {
            let mut parent = Page::new_raw();
            let guard =
                buffer_pool.fetch_page(PageKey::new(self.relation_id, self.root_page_id))?;

            parent
                .as_bytes_mut()
                .copy_from_slice(guard.page().as_bytes());
            parent
        };

        let left_page_id = buffer_pool.allocate_page(self.relation_id)?;
        let right_page_id = buffer_pool.allocate_page(self.relation_id)?;

        let mut left_page = Page::new_raw();
        let left_page_key = PageKey::new(self.relation_id, left_page_id);
        left_page.as_bytes_mut().copy_from_slice(&leaf_bytes);

        let mut right_page = Page::new_raw();
        let right_page_key = PageKey::new(self.relation_id, right_page_id);

        let separator = leaf_split(
            &mut left_page,
            &mut right_page,
            right_page_id,
            self.key_type,
            new_entry,
        )?;

        let replace_result = replace_child_after_leaf_split(
            &mut parent,
            self.key_type,
            old_child,
            left_page_id,
            right_page_id,
            separator.clone(),
        );

        match replace_result {
            Ok(()) => {
                Self::fetch_and_flush_page(buffer_pool, left_page_key, left_page.as_bytes())?;
                Self::fetch_and_flush_page(buffer_pool, right_page_key, right_page.as_bytes())?;
                Self::fetch_and_flush_page(
                    buffer_pool,
                    PageKey::new(self.relation_id, self.root_page_id),
                    parent.as_bytes(),
                )?;
            }
            Err(InternalPageError::PageFull) => {
                let left_internal_id = buffer_pool.allocate_page(self.relation_id)?;
                let right_internal_id = buffer_pool.allocate_page(self.relation_id)?;

                let mut left_internal_page = Page::new_raw();
                let left_internal_page_key = PageKey::new(self.relation_id, left_internal_id);
                left_internal_page
                    .as_bytes_mut()
                    .copy_from_slice(parent.as_bytes());

                let mut right_internal_page = Page::new_raw();
                let right_internal_page_key = PageKey::new(self.relation_id, right_internal_id);

                let root_separator = internal_split(
                    &mut left_internal_page,
                    &mut right_internal_page,
                    self.key_type,
                )?;

                match separator.compare(&root_separator) {
                    Some(Ordering::Less | Ordering::Equal) => {
                        replace_child_after_leaf_split(
                            &mut left_internal_page,
                            self.key_type,
                            old_child,
                            left_page_id,
                            right_page_id,
                            separator,
                        )?;
                    }
                    Some(Ordering::Greater) => {
                        replace_child_after_leaf_split(
                            &mut right_internal_page,
                            self.key_type,
                            old_child,
                            left_page_id,
                            right_page_id,
                            separator,
                        )?;
                    }
                    None => return Err(BTreeError::InvalidKeyType),
                }

                let new_root =
                    self.root_internal_split(left_internal_id, right_internal_id, root_separator)?;
                Self::fetch_and_flush_page(buffer_pool, left_page_key, left_page.as_bytes())?;
                Self::fetch_and_flush_page(buffer_pool, right_page_key, right_page.as_bytes())?;
                Self::fetch_and_flush_page(
                    buffer_pool,
                    left_internal_page_key,
                    left_internal_page.as_bytes(),
                )?;
                Self::fetch_and_flush_page(
                    buffer_pool,
                    right_internal_page_key,
                    right_internal_page.as_bytes(),
                )?;
                Self::fetch_and_flush_page(
                    buffer_pool,
                    PageKey::new(self.relation_id, self.root_page_id),
                    new_root.as_bytes(),
                )?;
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    fn root_internal_split(
        &self,
        left_internal_page_id: PageId,
        right_internal_page_id: PageId,
        root_separator: BTreeKey,
    ) -> Result<Page, BTreeError> {
        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_internal_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_internal_page_id, root_separator),
        )?;
        Ok(root_page)
    }
}
