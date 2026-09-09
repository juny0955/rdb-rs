use std::{collections::HashMap, path::PathBuf};

use thiserror::Error;

use crate::{
    catalog::metadata::RelationId,
    storage::{
        buffer::{BufferPool, BufferPoolError, FrameGuard, page_key::PageKey},
        file::{RelationFileManager, RelationFileManagerError},
        page::PageId,
    },
};

#[derive(Debug, Error)]
pub enum StorageManagerError {
    #[error(transparent)]
    BufferPool(#[from] BufferPoolError),
    #[error(transparent)]
    RelationFileManager(#[from] RelationFileManagerError),
}

pub struct StorageManager {
    buffer_pool: BufferPool,
    relation_file_manager: RelationFileManager,
    page_count: HashMap<RelationId, u64>,
}

impl StorageManager {
    pub fn new(buffer_pool_capacity: usize) -> Self {
        Self {
            buffer_pool: BufferPool::new(buffer_pool_capacity),
            relation_file_manager: RelationFileManager::new(),
            page_count: HashMap::new(),
        }
    }

    pub fn fetch_page(&mut self, page_key: PageKey) -> Result<FrameGuard<'_>, StorageManagerError> {
        if self.buffer_pool.contains_page(page_key) {
            return Ok(self.buffer_pool.fetch_cached_page(page_key)?);
        }

        if self.buffer_pool.is_full() {
            self.evict_one()?;
        }

        let page = self
            .relation_file_manager
            .read_page(page_key.relation_id(), page_key.page_id())?;
        Ok(self.buffer_pool.insert_page(page_key, page)?)
    }

    pub fn flush_page(&mut self, page_key: PageKey) -> Result<(), StorageManagerError> {
        if self.buffer_pool.is_dirty(page_key)? {
            {
                let guard = self.buffer_pool.fetch_cached_page(page_key)?;
                let page = guard.page();
                self.relation_file_manager.write_page(
                    page_key.relation_id(),
                    page_key.page_id(),
                    page,
                )?;
            }
            self.buffer_pool.clean_frame(page_key)?;
        }

        Ok(())
    }

    fn evict_one(&mut self) -> Result<(), StorageManagerError> {
        let page_key =
            self.buffer_pool
                .select_clock_victim()?
                .ok_or(StorageManagerError::BufferPool(
                    BufferPoolError::NoFreeFrame,
                ))?;
        self.flush_page(page_key)?;
        self.buffer_pool.remove_page(page_key)?;
        Ok(())
    }

    pub fn create_relation(
        &mut self,
        relation_id: RelationId,
        path: PathBuf,
    ) -> Result<(), StorageManagerError> {
        Ok(self
            .relation_file_manager
            .create_relation(relation_id, path)?)
    }

    pub fn register_relation(&mut self, relation_id: RelationId, path: PathBuf) {
        self.relation_file_manager
            .register_relation(relation_id, path)
    }

    pub fn allocate_page(
        &mut self,
        relation_id: RelationId,
    ) -> Result<PageId, StorageManagerError> {
        let page_id = self.relation_file_manager.allocate_page(relation_id)?;

        if let Some(count) = self.page_count.get_mut(&relation_id) {
            *count += 1;
        }

        Ok(page_id)
    }

    pub fn page_count(&mut self, relation_id: RelationId) -> Result<u64, StorageManagerError> {
        match self.page_count.get(&relation_id) {
            Some(count) => Ok(*count),
            None => {
                let count = self.relation_file_manager.page_count(relation_id)?;
                self.page_count.insert(relation_id, count);
                Ok(count)
            }
        }
    }
}

#[cfg(test)]
mod tests;
