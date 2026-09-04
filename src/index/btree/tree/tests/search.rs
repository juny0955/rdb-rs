use super::*;

#[test]
fn root_internal에서_left와_right_leaf를검색한다() -> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestFile::new("btree-search-root-internal");
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
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(BTreeKey::Int(10), left_row_id),
    )
    .expect("left leaf에 entry를 추가할 수 있어야 한다");

    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(BTreeKey::Int(50), right_row_id),
    )
    .expect("right leaf에 entry를 추가할 수 있어야 한다");

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    assert_eq!(allocate_page(&mut file)?, left_page_id);
    assert_eq!(allocate_page(&mut file)?, right_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    write_page(&mut file, left_page_id, &left_page)?;
    write_page(&mut file, right_page_id, &right_page)?;
    drop(file);

    let mut buffer_pool = BufferPool::new(2);
    buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

    assert_eq!(
        tree.search(&mut buffer_pool, BTreeKey::Int(10))?,
        vec![left_row_id]
    );
    assert_eq!(
        tree.search(&mut buffer_pool, BTreeKey::Int(50))?,
        vec![right_row_id]
    );
    assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(7))?, vec![]);
    Ok(())
}

#[test]
fn search은_다음_leaf의_중복_key까지_반환한다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let index_file = TestFile::new("btree-search-all-leaf-chain");
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
    )?;

    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(BTreeKey::Int(42), left_row_id),
    )?;
    let mut left_header = BTreePageHeader::read_from_page(&left_page)
        .expect("초기화한 left leaf header는 유효해야 한다");
    left_header.set_next_leaf_page_id(Some(right_page_id));
    left_header.write_to_page(&mut left_page);

    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(BTreeKey::Int(42), right_row_id),
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
    let mut reopened_buffer_pool = BufferPool::new(1);
    reopened_buffer_pool
        .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

    // When
    let row_ids = tree.search(&mut reopened_buffer_pool, BTreeKey::Int(42))?;

    // Then
    assert_eq!(row_ids, vec![left_row_id, right_row_id]);
    Ok(())
}
