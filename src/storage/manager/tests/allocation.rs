use super::*;
use crate::storage::file::RelationFileManagerError;

#[test]
fn 등록한_relation에_page를순서대로할당한다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = TestRelationFile::new("manager-allocation", "1.tbl");
    let mut storage_manager = storage_manager(&test_file, 1);

    let relation_id = RelationId::Heap(TableId::new(1));
    assert_eq!(storage_manager.allocate_page(relation_id)?, PageId::new(0));
    assert_eq!(storage_manager.allocate_page(relation_id)?, PageId::new(1));
    Ok(())
}

#[test]
fn 등록되지_않은_relation에_page를할당하면_오류다() {
    let directory = TestDirectory::new("manager-unregistered-allocation");
    let mut storage_manager = StorageManager::new(directory.path(), 1);
    let relation_id = RelationId::Heap(TableId::new(42));

    assert!(matches!(
        storage_manager.allocate_page(relation_id),
        Err(StorageManagerError::RelationFileManager(
            RelationFileManagerError::RelationNotRegistered(id)
        )) if id == relation_id
    ));
}
