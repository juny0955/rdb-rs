use super::*;
use crate::{
    catalog::metadata::{RelationId, TableId},
    storage::{
        buffer::page_key::PageKey,
        page::{
            Page, PageId, PagerError, Row, allocate_file_page as allocate_page, read_page,
            write_page,
        },
    },
};
use tempfile::NamedTempFile;

fn key(page_id: u64) -> PageKey {
    PageKey::new(RelationId::Heap(TableId::new(1)), PageId::new(page_id))
}

fn page_key(page_id: PageId) -> PageKey {
    PageKey::new(RelationId::Heap(TableId::new(1)), page_id)
}

fn storage_manager(test_file: &NamedTempFile, capacity: usize) -> StorageManager {
    let mut storage_manager = StorageManager::new(capacity);
    storage_manager.register_relation(
        RelationId::Heap(TableId::new(1)),
        test_file.path().to_path_buf(),
    );
    storage_manager
}

mod allocation;
mod eviction;
mod fetch;
mod flush;
