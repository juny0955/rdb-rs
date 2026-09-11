use super::*;

#[test]
fn root_leaf에서_entry를_삭제한_후_재시작해도_결과가_유지된다()
-> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestRelationFile::new("btree-delete-root-leaf", "1.idx");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let deleted_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let remaining_row_id = RowId::new(PageId::new(4), SlotId::new(8));
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
        tree.insert(&mut storage_manager, BTreeKey::Int(10), deleted_row_id)?;
        tree.insert(&mut storage_manager, BTreeKey::Int(20), remaining_row_id)?;

        assert!(tree.delete(&mut storage_manager, BTreeKey::Int(10), deleted_row_id)?);
        assert!(!tree.delete(&mut storage_manager, BTreeKey::Int(10), deleted_row_id)?);
        assert_eq!(
            tree.search(&mut storage_manager, BTreeKey::Int(10))?,
            vec![]
        );
        assert_eq!(
            tree.search(&mut storage_manager, BTreeKey::Int(20))?,
            vec![remaining_row_id]
        );
    }

    let mut reopened_buffer_pool = StorageManager::with_capacity(index_file.data_dir(), 1);
    reopened_buffer_pool.register_relation(RelationId::Index(index_id))?;
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
        vec![]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(20))?,
        vec![remaining_row_id]
    );
    Ok(())
}

#[test]
fn rightmost_leaf를_삭제하면_left_leaf와_병합되고_재시작후에도_유지된다()
-> Result<(), Box<dyn std::error::Error>> {
    // Given
    let index_file = TestRelationFile::new("btree-merge-rightmost-leaf", "1.idx");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let left_page_id = PageId::new(1);
    let right_page_id = PageId::new(2);
    let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let deleted_row_id = RowId::new(PageId::new(4), SlotId::new(8));

    let mut root_page = Page::new_raw();
    initialize_internal_page(&mut root_page, right_page_id);
    append_internal_entry(
        &mut root_page,
        &InternalEntry::new(left_page_id, BTreeKey::Int(10)),
    )?;

    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(BTreeKey::Int(10), left_row_id),
    )?;

    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(BTreeKey::Int(20), deleted_row_id),
    )?;

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
        let mut storage_manager = StorageManager::with_capacity(index_file.data_dir(), 1);
        storage_manager.register_relation(RelationId::Index(index_id))?;

        // When
        assert!(tree.delete(&mut storage_manager, BTreeKey::Int(20), deleted_row_id)?);

        // Then
        assert_eq!(
            tree.search(&mut storage_manager, BTreeKey::Int(10))?,
            vec![left_row_id]
        );
        assert_eq!(
            tree.search(&mut storage_manager, BTreeKey::Int(20))?,
            vec![]
        );
    }

    let mut reopened_buffer_pool = StorageManager::with_capacity(index_file.data_dir(), 1);
    reopened_buffer_pool.register_relation(RelationId::Index(index_id))?;
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(10))?,
        vec![left_row_id]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, BTreeKey::Int(20))?,
        vec![]
    );
    Ok(())
}
