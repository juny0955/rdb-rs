use std::cmp::Ordering;

use thiserror::Error;

use crate::index::header::BTreePageHeader;
use crate::index::internal::{
    InternalEntry, InternalPageError, append_internal_entry, find_internal_child,
    find_leaf_merge_pair, initialize_internal_page, internal_split,
    remove_right_child_after_leaf_merge, replace_child_after_leaf_split,
};
use crate::index::key::{BTreeKey, BTreeKeyType};
use crate::index::leaf::{
    LeafEntry, LeafPageError, append_leaf_entry, delete_leaf_entry, find_leaf_entry,
    find_leaf_row_ids, initialize_leaf_page, leaf_is_underfull, leaf_merge, leaf_split,
};
use crate::schema::{IndexId, RelationId};
use crate::storage::buffer::page_key::PageKey;
use crate::storage::buffer::{BufferPool, BufferPoolError};
use crate::storage::page::{Page, PageId, RowId};

#[derive(Debug, Error)]
pub enum BTreeError {
    #[error("key type이 일치하지 않습니다")]
    InvalidKeyType,
    #[error("BTree page가 손상되었습니다: {0:?}")]
    InvalidPage(PageId),
    #[error(transparent)]
    Buffer(#[from] BufferPoolError),
    #[error(transparent)]
    Leaf(#[from] LeafPageError),
    #[error(transparent)]
    Internal(#[from] InternalPageError),
}

#[derive(Debug)]
pub struct BTree {
    relation_id: RelationId,
    root_page_id: PageId,
    key_type: BTreeKeyType,
}

impl BTree {
    pub fn create(
        index_id: IndexId,
        key_type: BTreeKeyType,
        buffer_pool: &mut BufferPool,
    ) -> Result<Self, BTreeError> {
        let page_id = buffer_pool.allocate_page(RelationId::Index(index_id))?;

        let btree = Self::open(index_id, page_id, key_type);
        let page_key = PageKey::new(btree.relation_id, page_id);
        {
            let mut guard = buffer_pool.fetch_page(page_key)?;
            initialize_leaf_page(guard.page_mut());
        }

        buffer_pool.flush_page(page_key)?;
        Ok(btree)
    }

    pub fn open(index_id: IndexId, root_page_id: PageId, key_type: BTreeKeyType) -> Self {
        Self {
            relation_id: RelationId::Index(index_id),
            root_page_id,
            key_type,
        }
    }

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

    pub fn delete(
        &self,
        buffer_pool: &mut BufferPool,
        key: BTreeKey,
        row_id: RowId,
    ) -> Result<bool, BTreeError> {
        if !self.accepts_key(&key) {
            return Err(BTreeError::InvalidKeyType);
        }

        let (parent, page_id) = self.find_leaf_and_parent_page_ids(buffer_pool, &key)?;
        let page_key = PageKey::new(self.relation_id, page_id);
        let (deleted, is_underfull) = {
            let mut guard = buffer_pool.fetch_page(page_key)?;
            let page = guard.page_mut();

            let deleted = delete_leaf_entry(&mut *page, self.key_type, &key, row_id)?;
            let is_underfull = leaf_is_underfull(page)?;
            (deleted, is_underfull)
        };

        if deleted {
            buffer_pool.flush_page(page_key)?;

            if is_underfull && let Some(parent) = parent {
                let mut parent_page = Page::new_raw();
                let parent_key = PageKey::new(self.relation_id, parent);
                {
                    let guard = buffer_pool.fetch_page(PageKey::new(self.relation_id, parent))?;
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
                        let guard = buffer_pool.fetch_page(left_key)?;
                        left_page
                            .as_bytes_mut()
                            .copy_from_slice(guard.page().as_bytes());
                    }
                    {
                        let guard = buffer_pool.fetch_page(right_key)?;
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

                    Self::fetch_and_flush_page(buffer_pool, left_key, left_page.as_bytes())?;
                    Self::fetch_and_flush_page(buffer_pool, right_key, right_page.as_bytes())?;
                    Self::fetch_and_flush_page(buffer_pool, parent_key, parent_page.as_bytes())?;
                }
            }
        }

        Ok(deleted)
    }

    pub fn search(
        &self,
        buffer_pool: &mut BufferPool,
        target: BTreeKey,
    ) -> Result<Option<RowId>, BTreeError> {
        if !self.accepts_key(&target) {
            return Err(BTreeError::InvalidKeyType);
        }

        let page_id = self.find_leaf_page_id(buffer_pool, &target)?;
        let guard = buffer_pool.fetch_page(PageKey::new(self.relation_id, page_id))?;
        let page = guard.page();
        Ok(find_leaf_entry(page, self.key_type, &target)?)
    }

    pub fn search_all(
        &self,
        buffer_pool: &mut BufferPool,
        target: BTreeKey,
    ) -> Result<Vec<RowId>, BTreeError> {
        if !self.accepts_key(&target) {
            return Err(BTreeError::InvalidKeyType);
        }

        let mut results = Vec::new();
        let mut current_page_id = self.find_leaf_page_id(buffer_pool, &target)?;
        loop {
            let guard = buffer_pool.fetch_page(PageKey::new(self.relation_id, current_page_id))?;
            let page = guard.page();
            results.extend_from_slice(&find_leaf_row_ids(page, self.key_type, &target)?);

            let header = BTreePageHeader::read_from_page(page)
                .ok_or(BTreeError::InvalidPage(current_page_id))?;
            if let Some(next) = header.next_leaf_page_id() {
                current_page_id = next;
            } else {
                break;
            }
        }

        Ok(results)
    }

    pub fn root_page_id(&self) -> PageId {
        self.root_page_id
    }

    fn find_leaf_page_id(
        &self,
        buffer_pool: &mut BufferPool,
        target: &BTreeKey,
    ) -> Result<PageId, BTreeError> {
        let mut current_page_id = self.root_page_id;
        loop {
            let guard = buffer_pool.fetch_page(PageKey::new(self.relation_id, current_page_id))?;
            let page = guard.page();
            let header = BTreePageHeader::read_from_page(page)
                .ok_or(BTreeError::InvalidPage(current_page_id))?;
            if header.is_leaf() {
                return Ok(current_page_id);
            } else {
                current_page_id = find_internal_child(page, self.key_type, target)?;
            }
        }
    }

    fn find_leaf_and_parent_page_ids(
        &self,
        buffer_pool: &mut BufferPool,
        target: &BTreeKey,
    ) -> Result<(Option<PageId>, PageId), BTreeError> {
        let mut current_page_id = self.root_page_id;
        let mut parent = None;

        loop {
            let guard = buffer_pool.fetch_page(PageKey::new(self.relation_id, current_page_id))?;
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

    fn fetch_and_flush_page(
        buffer_pool: &mut BufferPool,
        page_key: PageKey,
        bytes: &[u8],
    ) -> Result<(), BTreeError> {
        buffer_pool
            .fetch_page(page_key)?
            .page_mut()
            .as_bytes_mut()
            .copy_from_slice(bytes);
        buffer_pool.flush_page(page_key)?;
        Ok(())
    }

    fn accepts_key(&self, key: &BTreeKey) -> bool {
        self.key_type == key.key_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        index::internal::{InternalEntry, append_internal_entry, initialize_internal_page},
        index::leaf::{LeafEntry, append_leaf_entry, initialize_leaf_page},
        storage::file::open_rw,
        storage::page::{Page, SlotId, allocate_page, write_page},
        test_supports::TestFile,
    };

    #[test]
    fn create는_disk에_빈_root_leaf를_생성한다() -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-create");
        let index_id = IndexId::new(1);
        let root_page_id = {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

            let tree = BTree::create(index_id, BTreeKeyType::Int, &mut buffer_pool)?;
            tree.root_page_id()
        };

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        let root_page = reopened_buffer_pool
            .fetch_page(PageKey::new(RelationId::Index(index_id), root_page_id))?;
        let header = BTreePageHeader::read_from_page(root_page.page())
            .expect("create가 초기화한 root page는 유효해야 함");

        assert_eq!(root_page_id, PageId::new(0));
        assert!(header.is_leaf());
        assert_eq!(header.entry_count(), 0);
        Ok(())
    }

    #[test]
    fn root_leaf의_부모는_없다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let index_file = TestFile::new("btree-find-root-leaf-parent");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let mut root_page = Page::new_raw();
        initialize_leaf_page(&mut root_page);

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        drop(file);

        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

        // When
        let page_ids = tree.find_leaf_and_parent_page_ids(&mut buffer_pool, &BTreeKey::Int(42))?;

        // Then
        assert_eq!(page_ids, (None, root_page_id));
        Ok(())
    }

    #[test]
    fn leaf의_직접_부모를_반환한다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let index_file = TestFile::new("btree-find-direct-leaf-parent");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let parent_page_id = PageId::new(1);
        let leaf_page_id = PageId::new(2);
        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, parent_page_id);
        let mut parent_page = Page::new_raw();
        initialize_internal_page(&mut parent_page, leaf_page_id);
        let mut leaf_page = Page::new_raw();
        initialize_leaf_page(&mut leaf_page);

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, parent_page_id);
        assert_eq!(allocate_page(&mut file)?, leaf_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, parent_page_id, &parent_page)?;
        write_page(&mut file, leaf_page_id, &leaf_page)?;
        drop(file);

        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

        // When
        let page_ids = tree.find_leaf_and_parent_page_ids(&mut buffer_pool, &BTreeKey::Int(42))?;

        // Then
        assert_eq!(page_ids, (Some(parent_page_id), leaf_page_id));
        Ok(())
    }

    #[test]
    fn root_leaf에서_key를검색한다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let index_file = TestFile::new("btree-search-root-leaf");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let mut root_page = Page::new_raw();
        initialize_leaf_page(&mut root_page);
        append_leaf_entry(&mut root_page, &LeafEntry::new(BTreeKey::Int(42), row_id))
            .expect("root leaf에 entry를 추가할 수 있어야 한다");

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        drop(file);

        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

        // When / Then
        assert_eq!(
            tree.search(&mut buffer_pool, BTreeKey::Int(42))?,
            Some(row_id)
        );
        assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(7))?, None);
        assert!(matches!(
            tree.search(&mut buffer_pool, BTreeKey::Varchar("42".to_owned())),
            Err(BTreeError::InvalidKeyType)
        ));
        Ok(())
    }

    #[test]
    fn root_leaf에_insert한_후_새_buffer_pool로_검색한다() -> Result<(), Box<dyn std::error::Error>>
    {
        let index_file = TestFile::new("btree-insert-root-leaf");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let mut root_page = Page::new_raw();
        initialize_leaf_page(&mut root_page);

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
            tree.insert(&mut buffer_pool, BTreeKey::Int(42), row_id)?;
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(42))?,
            Some(row_id)
        );
        Ok(())
    }

    #[test]
    fn root_leaf에서_entry를_삭제한_후_재시작해도_결과가_유지된다()
    -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-delete-root-leaf");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let deleted_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let remaining_row_id = RowId::new(PageId::new(4), SlotId::new(8));
        let mut root_page = Page::new_raw();
        initialize_leaf_page(&mut root_page);

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
            tree.insert(&mut buffer_pool, BTreeKey::Int(10), deleted_row_id)?;
            tree.insert(&mut buffer_pool, BTreeKey::Int(20), remaining_row_id)?;

            assert!(tree.delete(&mut buffer_pool, BTreeKey::Int(10), deleted_row_id)?);
            assert!(!tree.delete(&mut buffer_pool, BTreeKey::Int(10), deleted_row_id)?);
            assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(10))?, None);
            assert_eq!(
                tree.search(&mut buffer_pool, BTreeKey::Int(20))?,
                Some(remaining_row_id)
            );
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
            None
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(20))?,
            Some(remaining_row_id)
        );
        Ok(())
    }

    #[test]
    fn 가득찬_root_leaf를_split한_후_재시작해도_양쪽_key를_검색한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-root-leaf-split");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));
        let left_key = BTreeKey::Varchar(format!("000{}", "a".repeat(100)));
        let right_key = BTreeKey::Varchar(format!("999{}", "z".repeat(100)));

        let mut root_page = Page::new_raw();
        initialize_leaf_page(&mut root_page);
        for index in 0..71 {
            let key = BTreeKey::Varchar(format!("{index:03}{}", "a".repeat(100)));
            let row_id = if index == 0 {
                left_row_id
            } else {
                RowId::new(PageId::new(index + 10), SlotId::new(1))
            };
            append_leaf_entry(&mut root_page, &LeafEntry::new(key, row_id))?;
        }

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Varchar);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

            tree.insert(&mut buffer_pool, right_key.clone(), right_row_id)?;
            assert_eq!(
                tree.search(&mut buffer_pool, left_key.clone())?,
                Some(left_row_id)
            );
            assert_eq!(
                tree.search(&mut buffer_pool, right_key.clone())?,
                Some(right_row_id)
            );
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, left_key)?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, right_key)?,
            Some(right_row_id)
        );
        Ok(())
    }

    #[test]
    fn 가득찬_rightmost_leaf를_split한_후_재시작해도_세_범위를_검색한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-rightmost-leaf-split");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_page_id = PageId::new(1);
        let right_page_id = PageId::new(2);
        let left_key = BTreeKey::Varchar("m".to_owned());
        let split_left_key = BTreeKey::Varchar(format!("n000{}", "a".repeat(96)));
        let split_right_key = BTreeKey::Varchar("z".repeat(100));
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let split_left_row_id = RowId::new(PageId::new(4), SlotId::new(8));
        let split_right_row_id = RowId::new(PageId::new(5), SlotId::new(9));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_page_id, left_key.clone()),
        )?;

        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        append_leaf_entry(
            &mut left_page,
            &LeafEntry::new(left_key.clone(), left_row_id),
        )?;

        let mut right_page = Page::new_raw();
        initialize_leaf_page(&mut right_page);
        for index in 0..73 {
            let key = BTreeKey::Varchar(format!("n{index:03}{}", "a".repeat(96)));
            let row_id = if index == 0 {
                split_left_row_id
            } else {
                RowId::new(PageId::new(index + 10), SlotId::new(1))
            };
            append_leaf_entry(&mut right_page, &LeafEntry::new(key, row_id))?;
        }

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_page_id);
        assert_eq!(allocate_page(&mut file)?, right_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_page_id, &left_page)?;
        write_page(&mut file, right_page_id, &right_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Varchar);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

            tree.insert(
                &mut buffer_pool,
                split_right_key.clone(),
                split_right_row_id,
            )?;
            assert_eq!(
                tree.search(&mut buffer_pool, left_key.clone())?,
                Some(left_row_id)
            );
            assert_eq!(
                tree.search(&mut buffer_pool, split_left_key.clone())?,
                Some(split_left_row_id)
            );
            assert_eq!(
                tree.search(&mut buffer_pool, split_right_key.clone())?,
                Some(split_right_row_id)
            );
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, left_key)?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, split_left_key)?,
            Some(split_left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, split_right_key)?,
            Some(split_right_row_id)
        );
        Ok(())
    }

    #[test]
    fn 가득찬_root_internal을_split한_후_재시작해도_세_범위를_검색한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-root-internal-split");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_leaf_page_id = PageId::new(1);
        let full_rightmost_leaf_page_id = PageId::new(2);
        let left_key = BTreeKey::Varchar("m".to_owned());
        let split_left_key = BTreeKey::Varchar(format!("o000{}", "a".repeat(96)));
        let split_right_key = BTreeKey::Varchar("z".repeat(100));
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let split_left_row_id = RowId::new(PageId::new(4), SlotId::new(8));
        let split_right_row_id = RowId::new(PageId::new(5), SlotId::new(9));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, full_rightmost_leaf_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_leaf_page_id, left_key.clone()),
        )?;
        for index in 0..74 {
            append_internal_entry(
                &mut root_page,
                &InternalEntry::new(
                    PageId::new(index + 10),
                    BTreeKey::Varchar(format!("n{index:03}{}", "a".repeat(96))),
                ),
            )?;
        }

        let mut left_leaf_page = Page::new_raw();
        initialize_leaf_page(&mut left_leaf_page);
        append_leaf_entry(
            &mut left_leaf_page,
            &LeafEntry::new(left_key.clone(), left_row_id),
        )?;

        let mut full_rightmost_leaf_page = Page::new_raw();
        initialize_leaf_page(&mut full_rightmost_leaf_page);
        for index in 0..73 {
            let key = BTreeKey::Varchar(format!("o{index:03}{}", "a".repeat(96)));
            let row_id = if index == 0 {
                split_left_row_id
            } else {
                RowId::new(PageId::new(index + 100), SlotId::new(1))
            };
            append_leaf_entry(&mut full_rightmost_leaf_page, &LeafEntry::new(key, row_id))?;
        }

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_leaf_page_id);
        assert_eq!(allocate_page(&mut file)?, full_rightmost_leaf_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_leaf_page_id, &left_leaf_page)?;
        write_page(
            &mut file,
            full_rightmost_leaf_page_id,
            &full_rightmost_leaf_page,
        )?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Varchar);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

            tree.insert(
                &mut buffer_pool,
                split_right_key.clone(),
                split_right_row_id,
            )?;
            assert_eq!(
                tree.search(&mut buffer_pool, left_key.clone())?,
                Some(left_row_id)
            );
            assert_eq!(
                tree.search(&mut buffer_pool, split_left_key.clone())?,
                Some(split_left_row_id)
            );
            assert_eq!(
                tree.search(&mut buffer_pool, split_right_key.clone())?,
                Some(split_right_row_id)
            );
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, left_key)?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, split_left_key)?,
            Some(split_left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, split_right_key)?,
            Some(split_right_row_id)
        );
        Ok(())
    }

    #[test]
    fn root_internal의_양쪽_leaf에_insert한_후_재검색한다() -> Result<(), Box<dyn std::error::Error>>
    {
        let index_file = TestFile::new("btree-insert-root-internal");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_page_id = PageId::new(1);
        let right_page_id = PageId::new(2);
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_page_id, BTreeKey::Int(42)),
        )
        .expect("root internal에 child를 추가할 수 있어야 한다");

        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        let mut right_page = Page::new_raw();
        initialize_leaf_page(&mut right_page);

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_page_id);
        assert_eq!(allocate_page(&mut file)?, right_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_page_id, &left_page)?;
        write_page(&mut file, right_page_id, &right_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
        {
            let mut buffer_pool = BufferPool::new(2);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
            tree.insert(&mut buffer_pool, BTreeKey::Int(10), left_row_id)?;
            tree.insert(&mut buffer_pool, BTreeKey::Int(50), right_row_id)?;
        }

        let mut reopened_buffer_pool = BufferPool::new(2);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(50))?,
            Some(right_row_id)
        );
        Ok(())
    }

    #[test]
    fn rightmost_leaf를_삭제하면_left_leaf와_병합되고_재시작후에도_유지된다()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let index_file = TestFile::new("btree-merge-rightmost-leaf");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_page_id = PageId::new(1);
        let right_page_id = PageId::new(2);
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let deleted_row_id = RowId::new(PageId::new(4), SlotId::new(8));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_page_id, BTreeKey::Int(10)),
        )?;

        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        append_leaf_entry(
            &mut left_page,
            &LeafEntry::new(BTreeKey::Int(10), left_row_id),
        )?;

        let mut right_page = Page::new_raw();
        initialize_leaf_page(&mut right_page);
        append_leaf_entry(
            &mut right_page,
            &LeafEntry::new(BTreeKey::Int(20), deleted_row_id),
        )?;

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_page_id);
        assert_eq!(allocate_page(&mut file)?, right_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_page_id, &left_page)?;
        write_page(&mut file, right_page_id, &right_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
        {
            let mut buffer_pool = BufferPool::new(1);
            buffer_pool
                .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

            // When
            assert!(tree.delete(&mut buffer_pool, BTreeKey::Int(20), deleted_row_id)?);

            // Then
            assert_eq!(
                tree.search(&mut buffer_pool, BTreeKey::Int(10))?,
                Some(left_row_id)
            );
            assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(20))?, None);
        }

        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut reopened_buffer_pool, BTreeKey::Int(20))?,
            None
        );
        Ok(())
    }

    #[test]
    fn root_internal에서_left와_right_leaf를검색한다() -> Result<(), Box<dyn std::error::Error>> {
        let index_file = TestFile::new("btree-search-root-internal");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_page_id = PageId::new(1);
        let right_page_id = PageId::new(2);
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_page_id, BTreeKey::Int(42)),
        )
        .expect("root internal에 child를 추가할 수 있어야 한다");

        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        append_leaf_entry(
            &mut left_page,
            &LeafEntry::new(BTreeKey::Int(10), left_row_id),
        )
        .expect("left leaf에 entry를 추가할 수 있어야 한다");

        let mut right_page = Page::new_raw();
        initialize_leaf_page(&mut right_page);
        append_leaf_entry(
            &mut right_page,
            &LeafEntry::new(BTreeKey::Int(50), right_row_id),
        )
        .expect("right leaf에 entry를 추가할 수 있어야 한다");

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_page_id);
        assert_eq!(allocate_page(&mut file)?, right_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_page_id, &left_page)?;
        write_page(&mut file, right_page_id, &right_page)?;
        drop(file);

        let mut buffer_pool = BufferPool::new(2);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

        assert_eq!(
            tree.search(&mut buffer_pool, BTreeKey::Int(10))?,
            Some(left_row_id)
        );
        assert_eq!(
            tree.search(&mut buffer_pool, BTreeKey::Int(50))?,
            Some(right_row_id)
        );
        assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(7))?, None);
        Ok(())
    }

    #[test]
    fn search_all은_다음_leaf의_중복_key까지_반환한다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let index_file = TestFile::new("btree-search-all-leaf-chain");
        let index_id = IndexId::new(1);
        let root_page_id = PageId::new(0);
        let left_page_id = PageId::new(1);
        let right_page_id = PageId::new(2);
        let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));

        let mut root_page = Page::new_raw();
        initialize_internal_page(&mut root_page, right_page_id);
        append_internal_entry(
            &mut root_page,
            &InternalEntry::new(left_page_id, BTreeKey::Int(42)),
        )?;

        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        append_leaf_entry(
            &mut left_page,
            &LeafEntry::new(BTreeKey::Int(42), left_row_id),
        )?;
        let mut left_header = BTreePageHeader::read_from_page(&left_page)
            .expect("초기화한 left leaf header는 유효해야 한다");
        left_header.set_next_leaf_page_id(Some(right_page_id));
        left_header.write_to_page(&mut left_page);

        let mut right_page = Page::new_raw();
        initialize_leaf_page(&mut right_page);
        append_leaf_entry(
            &mut right_page,
            &LeafEntry::new(BTreeKey::Int(42), right_row_id),
        )?;

        let mut file = open_rw(index_file.path())?;
        assert_eq!(allocate_page(&mut file)?, root_page_id);
        assert_eq!(allocate_page(&mut file)?, left_page_id);
        assert_eq!(allocate_page(&mut file)?, right_page_id);
        write_page(&mut file, root_page_id, &root_page)?;
        write_page(&mut file, left_page_id, &left_page)?;
        write_page(&mut file, right_page_id, &right_page)?;
        drop(file);

        let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
        let mut reopened_buffer_pool = BufferPool::new(1);
        reopened_buffer_pool
            .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

        // When
        let row_ids = tree.search_all(&mut reopened_buffer_pool, BTreeKey::Int(42))?;

        // Then
        assert_eq!(row_ids, vec![left_row_id, right_row_id]);
        Ok(())
    }
}
