use super::*;
use crate::storage::file::RelationFileManagerError;

#[test]
fn dirty_frame을_flush하면_disk에_기록하고_clean으로_표시한다()
-> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-flush", "1.tbl");
    let mut file = test_file.reopen()?;
    let page_id = allocate_page(&mut file)?;
    let page_key = page_key(page_id);
    let row = Row::from_bytes(b"flush");
    let mut storage_manager = storage_manager(&test_file, 1);
    let slot_id = {
        let mut guard = storage_manager.fetch_page(page_key)?;
        guard.page_mut().insert_row(&row)?
    };

    storage_manager.flush_page(page_key)?;

    drop(file);
    let mut reopened_file = test_file.reopen()?;
    let persisted_page = read_page(&mut reopened_file, page_id)?;
    assert_eq!(persisted_page.read_row(slot_id)?, row);

    Ok(())
}

#[test]
fn flush가_실패하면_dirty_상태를_유지한다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-flush-failure", "1.tbl");
    let mut file = test_file.reopen()?;
    let page_id = allocate_page(&mut file)?;
    let page_key = page_key(page_id);
    let mut storage_manager = storage_manager(&test_file, 1);
    let row = Row::from_bytes(b"retry flush");
    let slot_id = {
        let mut guard = storage_manager.fetch_page(page_key)?;
        guard.page_mut().insert_row(&row)?
    };

    file.set_len(0)?;

    assert!(matches!(
        storage_manager.flush_page(page_key),
        Err(StorageManagerError::RelationFileManager(
            RelationFileManagerError::Pager(PagerError::PageNotAllocated(id))
        )) if id == page_id
    ));

    assert_eq!(allocate_page(&mut file)?, page_id);
    storage_manager.flush_page(page_key)?;
    let persisted_page = read_page(&mut file, page_id)?;
    assert_eq!(persisted_page.read_row(slot_id)?, row);

    Ok(())
}

#[test]
fn flush된_page는_storage_manager_재시작후에도_읽힌다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-flush-reopen", "1.tbl");
    let relation_id = RelationId::Heap(TableId::new(1));
    let row = Row::from_bytes(b"reopen persistence");

    let (page_key, slot_id) = {
        let mut storage_manager = storage_manager(&test_file, 1);
        let page_id = storage_manager.allocate_page(relation_id)?;
        let page_key = page_key(page_id);
        let slot_id = {
            let mut guard = storage_manager.fetch_page(page_key)?;
            guard.page_mut().insert_row(&row)?
        };

        storage_manager.flush_page(page_key)?;
        (page_key, slot_id)
    };

    let mut reopened_storage_manager = storage_manager(&test_file, 1);
    let guard = reopened_storage_manager.fetch_page(page_key)?;

    assert_eq!(guard.page().read_row(slot_id)?, row);
    Ok(())
}
