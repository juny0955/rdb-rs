use super::*;

#[test]
fn 가득찬_root_leaf를_split한_후_재시작해도_양쪽_key를_검색한다()
-> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestFile::new("btree-root-leaf-split");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let right_row_id = RowId::new(PageId::new(4), SlotId::new(8));
    let left_key = BTreeKey::Varchar(format!("000{}", "a".repeat(100)));
    let right_key = BTreeKey::Varchar(format!("999{}", "z".repeat(100)));

    let mut root_page = Page::new_raw();
    initialize_leaf_page(&mut root_page);
    for index in 0..71 {
        let key = BTreeKey::Varchar(format!("{index:03}{}", "a".repeat(100)));
        let row_id = if index == 0 {
            left_row_id
        } else {
            RowId::new(PageId::new(index + 10), SlotId::new(1))
        };
        append_leaf_entry(&mut root_page, &LeafEntry::new(key, row_id))?;
    }

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    drop(file);

    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Varchar);
    {
        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

        tree.insert(&mut buffer_pool, right_key.clone(), right_row_id)?;
        assert_eq!(
            tree.search(&mut buffer_pool, left_key.clone())?,
            vec![left_row_id]
        );
        assert_eq!(
            tree.search(&mut buffer_pool, right_key.clone())?,
            vec![right_row_id]
        );
    }

    let mut reopened_buffer_pool = BufferPool::new(1);
    reopened_buffer_pool
        .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, left_key)?,
        vec![left_row_id]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, right_key)?,
        vec![right_row_id]
    );
    Ok(())
}

#[test]
fn 가득찬_rightmost_leaf를_split한_후_재시작해도_세_범위를_검색한다()
-> Result<(), Box<dyn std::error::Error>> {
    let index_file = TestFile::new("btree-rightmost-leaf-split");
    let index_id = IndexId::new(1);
    let root_page_id = PageId::new(0);
    let left_page_id = PageId::new(1);
    let right_page_id = PageId::new(2);
    let left_key = BTreeKey::Varchar("m".to_owned());
    let split_left_key = BTreeKey::Varchar(format!("n000{}", "a".repeat(96)));
    let split_right_key = BTreeKey::Varchar("z".repeat(100));
    let left_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let split_left_row_id = RowId::new(PageId::new(4), SlotId::new(8));
    let split_right_row_id = RowId::new(PageId::new(5), SlotId::new(9));

    let mut root_page = Page::new_raw();
    initialize_internal_page(&mut root_page, right_page_id);
    append_internal_entry(
        &mut root_page,
        &InternalEntry::new(left_page_id, left_key.clone()),
    )?;

    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(left_key.clone(), left_row_id),
    )?;

    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    for index in 0..73 {
        let key = BTreeKey::Varchar(format!("n{index:03}{}", "a".repeat(96)));
        let row_id = if index == 0 {
            split_left_row_id
        } else {
            RowId::new(PageId::new(index + 10), SlotId::new(1))
        };
        append_leaf_entry(&mut right_page, &LeafEntry::new(key, row_id))?;
    }

    let mut file = open_rw(index_file.path())?;
    assert_eq!(allocate_page(&mut file)?, root_page_id);
    assert_eq!(allocate_page(&mut file)?, left_page_id);
    assert_eq!(allocate_page(&mut file)?, right_page_id);
    write_page(&mut file, root_page_id, &root_page)?;
    write_page(&mut file, left_page_id, &left_page)?;
    write_page(&mut file, right_page_id, &right_page)?;
    drop(file);

    let tree = BTree::open(index_id, root_page_id, BTreeKeyType::Varchar);
    {
        let mut buffer_pool = BufferPool::new(1);
        buffer_pool.register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());

        tree.insert(
            &mut buffer_pool,
            split_right_key.clone(),
            split_right_row_id,
        )?;
        assert_eq!(
            tree.search(&mut buffer_pool, left_key.clone())?,
            vec![left_row_id]
        );
        assert_eq!(
            tree.search(&mut buffer_pool, split_left_key.clone())?,
            vec![split_left_row_id]
        );
        assert_eq!(
            tree.search(&mut buffer_pool, split_right_key.clone())?,
            vec![split_right_row_id]
        );
    }

    let mut reopened_buffer_pool = BufferPool::new(1);
    reopened_buffer_pool
        .register_relation(RelationId::Index(index_id), index_file.path().to_path_buf());
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, left_key)?,
        vec![left_row_id]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, split_left_key)?,
        vec![split_left_row_id]
    );
    assert_eq!(
        tree.search(&mut reopened_buffer_pool, split_right_key)?,
        vec![split_right_row_id]
    );
    Ok(())
}
