use super::*;
use crate::{
    catalog::metadata::{RelationId, TableId},
    storage::{
        buffer::page_key::PageKey,
        file::open_rw_create,
        page::{
            Page, PageId, PagerError, Row, allocate_file_page as allocate_page, read_page,
            write_page,
        },
    },
    test_supports::{TestDirectory, TestRelationFile},
};

fn key(page_id: u64) -> PageKey {
    PageKey::new(RelationId::Heap(TableId::new(1)), PageId::new(page_id))
}

fn page_key(page_id: PageId) -> PageKey {
    PageKey::new(RelationId::Heap(TableId::new(1)), page_id)
}

fn storage_manager(test_file: &TestRelationFile, capacity: usize) -> StorageManager {
    let mut storage_manager = StorageManager::new(test_file.data_dir(), capacity);
    storage_manager
        .register_relation(RelationId::Heap(TableId::new(1)))
        .expect("테스트 relation을 등록해야 함");
    storage_manager
}

mod allocation;
mod eviction;
mod fetch;
mod flush;
