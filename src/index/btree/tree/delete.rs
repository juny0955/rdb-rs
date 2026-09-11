use crate::{
    index::btree::{
        BTreeKey,
        header::BTreePageHeader,
        internal::{
            find_internal_child, find_leaf_merge_pair, remove_right_child_after_leaf_merge,
        },
        leaf::{delete_leaf_entry, leaf_is_underfull, leaf_merge},
        tree::{BTree, BTreeError},
    },
    storage::{
        StorageManager,
        buffer::page_key::PageKey,
        page::{Page, PageId, RowId},
    },
};

impl BTree {
    pub fn delete(
        &self,
        storage_manager: &mut StorageManager,
        key: BTreeKey,
        row_id: RowId,
    ) -> Result<bool, BTreeError> {
        if !self.accepts_key(&key) {
            return Err(BTreeError::InvalidKeyType);
        }

        let (parent, page_id) = self.find_leaf_and_parent_page_ids(storage_manager, &key)?;
        let page_key = PageKey::new(self.relation_id, page_id);
        let (deleted, is_underfull) = {
            let mut guard = storage_manager.fetch_page(page_key)?;
            let page = guard.page_mut();

            let deleted = delete_leaf_entry(&mut *page, self.key_type, &key, row_id)?;
            let is_underfull = leaf_is_underfull(page)?;
            (deleted, is_underfull)
        };

        if deleted {
            storage_manager.flush_page(page_key)?;

            if is_underfull && let Some(parent) = parent {
                let mut parent_page = Page::new_raw();
                let parent_key = PageKey::new(self.relation_id, parent);
                {
                    let guard =
                        storage_manager.fetch_page(PageKey::new(self.relation_id, parent))?;
                    parent_page
                        .as_bytes_mut()
                        .copy_from_slice(guard.page().as_bytes());
                }

                if let Some((left, right)) =
                    find_leaf_merge_pair(&parent_page, self.key_type, page_id)?
                {
                    let left_key = PageKey::new(self.relation_id, left);
                    let right_key = PageKey::new(self.relation_id, right);
                    let mut left_page = Page::new_raw();
                    let mut right_page = Page::new_raw();
                    {
                        let guard = storage_manager.fetch_page(left_key)?;
                        left_page
                            .as_bytes_mut()
                            .copy_from_slice(guard.page().as_bytes());
                    }
                    {
                        let guard = storage_manager.fetch_page(right_key)?;
                        right_page
                            .as_bytes_mut()
                            .copy_from_slice(guard.page().as_bytes());
                    }

                    leaf_merge(&mut left_page, &mut right_page, self.key_type)?;
                    remove_right_child_after_leaf_merge(
                        &mut parent_page,
                        left,
                        right,
                        self.key_type,
                    )?;

                    Self::fetch_and_flush_page(storage_manager, left_key, left_page.as_bytes())?;
                    Self::fetch_and_flush_page(storage_manager, right_key, right_page.as_bytes())?;
                    Self::fetch_and_flush_page(
                        storage_manager,
                        parent_key,
                        parent_page.as_bytes(),
                    )?;
                }
            }
        }

        Ok(deleted)
    }

    pub(super) fn find_leaf_and_parent_page_ids(
        &self,
        storage_manager: &mut StorageManager,
        target: &BTreeKey,
    ) -> Result<(Option<PageId>, PageId), BTreeError> {
        let mut current_page_id = self.root_page_id;
        let mut parent = None;

        loop {
            let guard =
                storage_manager.fetch_page(PageKey::new(self.relation_id, current_page_id))?;
            let page = guard.page();
            let header = BTreePageHeader::read_from_page(page)
                .ok_or(BTreeError::InvalidPage(current_page_id))?;
            if header.is_leaf() {
                return Ok((parent, current_page_id));
            } else {
                parent = Some(current_page_id);
                current_page_id = find_internal_child(page, self.key_type, target)?;
            }
        }
    }
}
