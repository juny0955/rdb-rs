use thiserror::Error;

use crate::catalog::metadata::{IndexId, RelationId};
use crate::index::btree::header::BTreePageHeader;
use crate::index::btree::internal::{InternalPageError, find_internal_child};
use crate::index::btree::key::{BTreeKey, BTreeKeyType};
use crate::index::btree::leaf::{LeafPageError, find_leaf_row_ids, initialize_leaf_page};
use crate::storage::buffer::page_key::PageKey;
use crate::storage::buffer::{BufferPool, BufferPoolError};
use crate::storage::page::{PageId, RowId};

mod delete;
mod insert;

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

    pub fn search(
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

    pub fn root_page_id(&self) -> PageId {
        self.root_page_id
    }
}

#[cfg(test)]
#[path = "tree/tests/mod.rs"]
mod tests;
