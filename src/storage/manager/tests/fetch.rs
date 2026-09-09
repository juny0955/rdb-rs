use super::*;
use crate::storage::file::RelationFileManagerError;

#[test]
fn 등록되지_않은_table의_page를_fetch하면_오류다() {
    let mut storage_manager = StorageManager::new(1);

    assert!(matches!(
        storage_manager.fetch_page(key(0)),
        Err(StorageManagerError::RelationFileManager(
            RelationFileManagerError::RelationNotRegistered(id)
        ))
            if id == RelationId::Heap(TableId::new(1))
    ));
}

#[test]
fn 다른_table의_같은_page_id는_서로_다른_frame에_저장된다() -> Result<(), Box<dyn std::error::Error>>
{
    let first_table_id = TableId::new(1);
    let second_table_id = TableId::new(2);
    let first_file = NamedTempFile::new()?;
    let second_file = NamedTempFile::new()?;
    let mut first_table_file = first_file.reopen()?;
    let mut second_table_file = second_file.reopen()?;
    let page_id = allocate_page(&mut first_table_file)?;
    assert_eq!(allocate_page(&mut second_table_file)?, page_id);

    let first_row = Row::from_bytes(b"first table");
    let mut first_page = Page::new();
    let first_slot_id = first_page.insert_row(&first_row)?;
    write_page(&mut first_table_file, page_id, &first_page)?;

    let second_row = Row::from_bytes(b"second table");
    let mut second_page = Page::new();
    let second_slot_id = second_page.insert_row(&second_row)?;
    write_page(&mut second_table_file, page_id, &second_page)?;

    let first_relation = RelationId::Heap(first_table_id);
    let second_relation = RelationId::Heap(second_table_id);
    let first_key = PageKey::new(first_relation, page_id);
    let second_key = PageKey::new(second_relation, page_id);
    let mut storage_manager = StorageManager::new(2);
    storage_manager.register_relation(first_relation, first_file.path().to_path_buf());
    storage_manager.register_relation(second_relation, second_file.path().to_path_buf());

    {
        let first_frame = storage_manager.fetch_page(first_key)?;
        assert_eq!(first_frame.page().read_row(first_slot_id)?, first_row);
    }
    {
        let second_frame = storage_manager.fetch_page(second_key)?;
        assert_eq!(second_frame.page().read_row(second_slot_id)?, second_row);
    }

    Ok(())
}

#[test]
fn 다른_table의_dirty_victim을_evict하면_원래_file에_기록한다()
-> Result<(), Box<dyn std::error::Error>> {
    let first_table_id = TableId::new(1);
    let second_table_id = TableId::new(2);
    let first_file = NamedTempFile::new()?;
    let second_file = NamedTempFile::new()?;
    let mut first_table_file = first_file.reopen()?;
    let mut second_table_file = second_file.reopen()?;
    let page_id = allocate_page(&mut first_table_file)?;
    assert_eq!(allocate_page(&mut second_table_file)?, page_id);

    let first_relation = RelationId::Heap(first_table_id);
    let second_relation = RelationId::Heap(second_table_id);
    let first_key = PageKey::new(first_relation, page_id);
    let second_key = PageKey::new(second_relation, page_id);
    let mut storage_manager = StorageManager::new(1);
    storage_manager.register_relation(first_relation, first_file.path().to_path_buf());
    storage_manager.register_relation(second_relation, second_file.path().to_path_buf());

    let row = Row::from_bytes(b"dirty victim");
    let slot_id = {
        let mut first_frame = storage_manager.fetch_page(first_key)?;
        first_frame.page_mut().insert_row(&row)?
    };

    {
        let _second_frame = storage_manager.fetch_page(second_key)?;
    }

    let persisted_page = read_page(&mut first_table_file, page_id)?;
    assert_eq!(persisted_page.read_row(slot_id)?, row);
    Ok(())
}

#[test]
fn 같은_page를_다시_fetch하면_disk_변경을읽지않는다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = NamedTempFile::new()?;
    let mut file = test_file.reopen()?;
    let page_id = allocate_page(&mut file)?;
    let page_key = page_key(page_id);
    let cached_row = Row::from_bytes(b"cached row");
    let mut cached_page = Page::new();
    let slot_id = cached_page.insert_row(&cached_row)?;
    write_page(&mut file, page_id, &cached_page)?;

    let mut storage_manager = storage_manager(&test_file, 1);
    {
        let guard = storage_manager.fetch_page(page_key)?;
        assert_eq!(guard.page().read_row(slot_id)?, cached_row);
    }

    let disk_row = Row::from_bytes(b"changed on disk");
    let mut changed_page = Page::new();
    assert_eq!(changed_page.insert_row(&disk_row)?, slot_id);
    write_page(&mut file, page_id, &changed_page)?;

    let guard = storage_manager.fetch_page(page_key)?;
    assert_eq!(guard.page().read_row(slot_id)?, cached_row);
    Ok(())
}
