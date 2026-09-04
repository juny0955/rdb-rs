use super::*;

#[test]
fn create는_disk에_빈_root_leaf를_생성한다() -> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestFile::new("btree-create");
    let index_id = IndexId::new(1);
    let root_page_id = {
        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

        let tree = BTree::create(index_id, BTreeKeyType::Int, &mut buffer_pool)?;
        tree.root_page_id()
    };

    let mut reopened_buffer_pool = BufferPool::new(1);
    reopened_buffer_pool
        .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    let root_page =
        reopened_buffer_pool.fetch_page(PageKey::new(RelationId::Index(index_id), root_page_id))?;
    let header = BTreePageHeader::read_from_page(root_page.page())
        .expect("create가 초기화한 root page는 유효해야 함");

    assert_eq!(root_page_id, PageId::new(0));
    assert!(header.is_leaf());
    assert_eq!(header.entry_count(), 0);
    Ok(())
}

#[test]
fn root_leaf의_부모는_없다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let index_file = TestFile::new("btree-find-root-leaf-parent");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let mut root_page = Page::new_raw();
    initialize_leaf_page(&mut root_page);

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    drop(file);

    let mut buffer_pool = BufferPool::new(1);
    buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

    // When
    let page_ids = tree.find_leaf_and_parent_page_ids(&mut buffer_pool, &BTreeKey::Int(42))?;

    // Then
    assert_eq!(page_ids, (None, root_page_id));
    Ok(())
}

#[test]
fn leaf의_직접_부모를_반환한다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let index_file = TestFile::new("btree-find-direct-leaf-parent");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let parent_page_id = PageId::new(1);
    let leaf_page_id = PageId::new(2);
    let mut root_page = Page::new_raw();
    initialize_internal_page(&mut root_page, parent_page_id);
    let mut parent_page = Page::new_raw();
    initialize_internal_page(&mut parent_page, leaf_page_id);
    let mut leaf_page = Page::new_raw();
    initialize_leaf_page(&mut leaf_page);

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    assert_eq!(allocate_page(&mut file)?, parent_page_id);
    assert_eq!(allocate_page(&mut file)?, leaf_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    write_page(&mut file, parent_page_id, &parent_page)?;
    write_page(&mut file, leaf_page_id, &leaf_page)?;
    drop(file);

    let mut buffer_pool = BufferPool::new(1);
    buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

    // When
    let page_ids = tree.find_leaf_and_parent_page_ids(&mut buffer_pool, &BTreeKey::Int(42))?;

    // Then
    assert_eq!(page_ids, (Some(parent_page_id), leaf_page_id));
    Ok(())
}

#[test]
fn root_leaf에서_key를검색한다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let index_file = TestFile::new("btree-search-root-leaf");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let mut root_page = Page::new_raw();
    initialize_leaf_page(&mut root_page);
    append_leaf_entry(&mut root_page, &LeafEntry::new(BTreeKey::Int(42), row_id))
        .expect("root leaf에 entry를 추가할 수 있어야 한다");

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    drop(file);

    let mut buffer_pool = BufferPool::new(1);
    buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Int);

    // When / Then
    assert_eq!(
        tree.search(&mut buffer_pool, BTreeKey::Int(42))?,
        vec![row_id]
    );
    assert_eq!(tree.search(&mut buffer_pool, BTreeKey::Int(7))?, vec![]);
    assert!(matches!(
        tree.search(&mut buffer_pool, BTreeKey::Varchar("42".to_owned())),
        Err(BTreeError::InvalidKeyType)
    ));
    Ok(())
}
