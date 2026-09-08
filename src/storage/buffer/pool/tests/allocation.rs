use super::*;
use crate::storage::file::RelationFileManagerError;

#[test]
fn 등록한_relation에_page를순서대로할당한다() -> Result<(), Box<dyn std::error::Error>> {
    let test_file = NamedTempFile::new()?;
    let mut pool = BufferPool::new(1);
    register_test_file(&mut pool, &test_file);

    let relation_id = RelationId::Heap(TableId::new(1));
    assert_eq!(pool.allocate_page(relation_id)?, PageId::new(0));
    assert_eq!(pool.allocate_page(relation_id)?, PageId::new(1));
    Ok(())
}

#[test]
fn 등록되지_않은_relation에_page를할당하면_오류다() {
    let mut pool = BufferPool::new(1);
    let relation_id = RelationId::Heap(TableId::new(42));

    assert!(matches!(
        pool.allocate_page(relation_id),
        Err(BufferPoolError::RelationFileManager(
            RelationFileManagerError::RelationNotRegistered(id)
        )) if id == relation_id
    ));
}
