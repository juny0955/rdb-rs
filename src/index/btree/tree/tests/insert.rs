use super::*;

#[test]
fn root_leaf에_insert한_후_새_buffer_pool로_검색한다() -> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestRelationFile::new("btree-insert-root-leaf", "1.idx");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let mut root_page = Page::new_raw();
    initialize_leaf_page(&mut root_page);

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    drop(file);

    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
    {
        let mut storage_manager = StorageManager::with_capacity(index_file.data_dir(), 1);
        storage_manager.register_relation(RelationId::Index(index_id))?;
        tree.insert(&mut storage_manager, BTreeKey::Int(42), row_id)?;
    }

    let mut reopened_buffer_pool = StorageManager::with_capacity(index_file.data_dir(), 1);
    reopened_buffer_pool.register_relation(RelationId::Index(index_id))?;
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(42))?,
        vec![row_id]
    );
    Ok(())
}

#[test]
fn root_internal의_양쪽_leaf에_insert한_후_재검색한다() -> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestRelationFile::new("btree-insert-root-internal", "1.idx");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let left_page_id = PageId::new(1);
    let right_page_id = PageId::new(2);
    let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));

    let mut root_page = Page::new_raw();
    initialize_internal_page(&mut root_page, right_page_id);
    append_internal_entry(
        &mut root_page,
        &InternalEntry::new(left_page_id, BTreeKey::Int(42)),
    )
    .expect("root internal에 child를 추가할 수 있어야 한다");

    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    assert_eq!(allocate_page(&mut file)?, left_page_id);
    assert_eq!(allocate_page(&mut file)?, right_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    write_page(&mut file, left_page_id, &left_page)?;
    write_page(&mut file, right_page_id, &right_page)?;
    drop(file);

    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);
    {
        let mut storage_manager = StorageManager::with_capacity(index_file.data_dir(), 2);
        storage_manager.register_relation(RelationId::Index(index_id))?;
        tree.insert(&mut storage_manager, BTreeKey::Int(10), left_row_id)?;
        tree.insert(&mut storage_manager, BTreeKey::Int(50), right_row_id)?;
    }

    let mut reopened_buffer_pool = StorageManager::with_capacity(index_file.data_dir(), 2);
    reopened_buffer_pool.register_relation(RelationId::Index(index_id))?;
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
        vec![left_row_id]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(50))?,
        vec![right_row_id]
    );
    Ok(())
}
