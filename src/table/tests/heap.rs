use crate::{
    catalog::metadata::{RelationId, TableId},
    storage::{
        StorageManager,
        page::{PageError, Row},
    },
    table::{TableError, heap::HeapTable},
    test_supports::TestRelationFile,
};

fn table_id() -> TableId {
    TableId::new(1)
}

fn storage_manager(test_file: &TestRelationFile) -> StorageManager {
    let mut storage_manager = StorageManager::with_capacity(test_file.data_dir(), 16);
    storage_manager
        .register_relation(RelationId::Heap(table_id()))
        .expect("테이블 relation을 등록해야 함");
    storage_manager
}

#[test]
fn new_테스트() {
    let _ = HeapTable::new(table_id());
}

#[test]
fn add_page_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("add-page", "1.tbl");
    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let page_id1 = table.add_page(&mut storage_manager)?;
    let page_id2 = table.add_page(&mut storage_manager)?;
    assert_ne!(page_id1, page_id2);
    assert_eq!(storage_manager.page_count(RelationId::Heap(table_id()))?, 2);
    Ok(())
}

#[test]
fn insert_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("insert", "1.tbl");
    let row = Row::from_bytes(&[1, 2, 3]);
    let row_id;
    {
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        row_id = table.insert(&row, &mut storage_manager)?;
    }
    {
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        assert_eq!(table.get(&mut storage_manager, row_id)?, row);
    }
    Ok(())
}

#[test]
fn insert_storage_full_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("insert_storage_full", "1.tbl");
    let row1 = Row::from_bytes(&vec![1; 8000]);
    let row2 = Row::from_bytes(&[1; 200]);
    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let row_id1 = table.insert(&row1, &mut storage_manager)?;
    let row_id2 = table.insert(&row2, &mut storage_manager)?;

    assert_ne!(row_id1, row_id2);
    assert_eq!(storage_manager.page_count(RelationId::Heap(table_id()))?, 2);
    Ok(())
}

#[test]
fn get_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("get", "1.tbl");
    let row = Row::from_bytes(&[1; 200]);
    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let row_id = table.insert(&row, &mut storage_manager)?;

    let read = table.get(&mut storage_manager, row_id)?;

    assert_eq!(row, read);
    Ok(())
}

#[test]
fn update_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("update", "1.tbl");

    let row = Row::from_bytes(&[1, 2, 3]);
    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let row_id = table.insert(&row, &mut storage_manager)?;

    let update = Row::from_bytes(&[4, 5, 6]);
    table.update(&mut storage_manager, row_id, &update)?;
    let updated_row = table.get(&mut storage_manager, row_id)?;

    assert_ne!(row, updated_row);
    assert_eq!(update, updated_row);
    Ok(())
}

#[test]
fn update_재시작_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("update-reopen", "1.tbl");
    let row = Row::from_bytes(&[1, 2, 3]);
    let updated_row = Row::from_bytes(&[4, 5, 6]);

    let row_id = {
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id = table.insert(&row, &mut storage_manager)?;
        table.update(&mut storage_manager, row_id, &updated_row)?;
        row_id
    };

    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);

    assert_eq!(table.get(&mut storage_manager, row_id)?, updated_row);
    Ok(())
}

#[test]
fn delete_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("delete", "1.tbl");
    let row = Row::from_bytes(&[1, 2, 3]);
    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let row_id = table.insert(&row, &mut storage_manager)?;
    let get = table.get(&mut storage_manager, row_id)?;
    assert_eq!(get, row);

    table.delete(&mut storage_manager, row_id)?;
    let error = table
        .get(&mut storage_manager, row_id)
        .expect_err("not found");
    assert!(matches!(error, TableError::Page(PageError::SlotNotFound)));
    Ok(())
}

#[test]
fn delete_재시작_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("delete-reopen", "1.tbl");
    let row = Row::from_bytes(&[1, 2, 3]);

    let row_id = {
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id = table.insert(&row, &mut storage_manager)?;
        table.delete(&mut storage_manager, row_id)?;
        row_id
    };

    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let error = table
        .get(&mut storage_manager, row_id)
        .expect_err("삭제한 row는 재시작 뒤에도 없어야 함");

    assert!(matches!(error, TableError::Page(PageError::SlotNotFound)));
    Ok(())
}

#[test]
fn scan_재시작_테스트() -> Result<(), TableError> {
    let test_file = TestRelationFile::new("scan-reopen", "1.tbl");
    let row1 = Row::from_bytes(&vec![1; 8000]);
    let row2 = Row::from_bytes(&[2; 200]);
    let row3 = Row::from_bytes(&[3; 200]);

    let (row_id1, row_id3) = {
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id1 = table.insert(&row1, &mut storage_manager)?;
        let row_id2 = table.insert(&row2, &mut storage_manager)?;
        let row_id3 = table.insert(&row3, &mut storage_manager)?;
        table.delete(&mut storage_manager, row_id2)?;
        (row_id1, row_id3)
    };

    let mut table = HeapTable::new(table_id());
    let mut storage_manager = storage_manager(&test_file);
    let scans = table.scan(&mut storage_manager)?;

    assert_eq!(scans, vec![(row_id1, row1), (row_id3, row3)]);
    Ok(())
}
