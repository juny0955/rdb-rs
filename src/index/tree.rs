use std::cmp::Ordering;

use thiserror::Error;

use crate::buffer::page_key::PageKey;
use crate::buffer::{BufferPool, BufferPoolError};
use crate::index::header::BTreePageHeader;
use crate::index::internal::{
    InternalEntry, InternalPageError, append_internal_entry, find_internal_child,
    initialize_internal_page, internal_split, replace_child_after_leaf_split,
};
use crate::index::key::{BTreeKey, BTreeKeyType};
use crate::index::leaf::{
    LeafEntry, LeafPageError, append_leaf_entry, find_leaf_entry, leaf_split,
};
use crate::page::{Page, PageId, RowId};
use crate::schema::{IndexId, RelationId};

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
    pub fn new(index_id: IndexId, root_page_id: PageId, key_type: BTreeKeyType) -> Self {
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

        let separator = leaf_split(&mut left_page, &mut right_page, self.key_type, new_entry)?;
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

        let separator = leaf_split(&mut left_page, &mut right_page, self.key_type, new_entry)?;

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
        file::open_rw,
        index::internal::{InternalEntry, append_internal_entry, initialize_internal_page},
        index::leaf::{LeafEntry, append_leaf_entry, initialize_leaf_page},
        page::{Page, SlotId, allocate_page, write_page},
        test_supports::TestFile,
    };

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
        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Int);

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

        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Int);
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

        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Varchar);
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

        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Varchar);
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

        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Varchar);
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

        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Int);
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
        let tree = BTree::new(index_id, root_page_id, BTreeKeyType::Int);

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
}
