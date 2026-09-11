use super::*;
use crate::storage::file::RelationFileManagerError;

#[test]
fn dirty_unpinned_page를_evict하면_disk에_기록한다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-eviction", "1.tbl");
    let mut file = test_file.reopen()?;
    let first_page_id = allocate_page(&mut file)?;
    let second_page_id = allocate_page(&mut file)?;
    let first_page_key = page_key(first_page_id);
    let second_page_key = page_key(second_page_id);
    let row = Row::from_bytes(b"evict");
    let mut storage_manager = storage_manager(&test_file, 1);
    let slot_id = {
        let mut guard = storage_manager.fetch_page(first_page_key)?;
        guard.page_mut().insert_row(&row)?
    };

    let second_guard = storage_manager.fetch_page(second_page_key)?;
    drop(second_guard);

    drop(file);
    let mut reopened_file = test_file.reopen()?;
    let persisted_page = read_page(&mut reopened_file, first_page_id)?;
    assert_eq!(persisted_page.read_row(slot_id)?, row);

    Ok(())
}

#[test]
fn dirty_page의_flush가_실패하면_evict하지_않는다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-eviction-failure", "1.tbl");
    let mut file = test_file.reopen()?;
    let first_page_id = allocate_page(&mut file)?;
    let second_page_id = allocate_page(&mut file)?;
    let first_page_key = page_key(first_page_id);
    let second_page_key = page_key(second_page_id);
    let row = Row::from_bytes(b"retain dirty page");
    let mut storage_manager = storage_manager(&test_file, 1);
    let slot_id = {
        let mut guard = storage_manager.fetch_page(first_page_key)?;
        guard.page_mut().insert_row(&row)?
    };

    file.set_len(0)?;

    assert!(matches!(
        storage_manager.fetch_page(second_page_key),
        Err(StorageError::RelationFileManager(
            RelationFileManagerError::Pager(PagerError::PageNotAllocated(id))
        )) if id == first_page_id
    ));

    assert_eq!(allocate_page(&mut file)?, first_page_id);
    storage_manager.flush_page(first_page_key)?;
    let persisted_page = read_page(&mut file, first_page_id)?;
    assert_eq!(persisted_page.read_row(slot_id)?, row);

    Ok(())
}
