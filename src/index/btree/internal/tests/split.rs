use super::*;

#[test]
fn internal_page를_split하고_중간_separator를반환한다() -> Result<(), Box<dyn std::error::Error>> {
    let mut left_page = Page::new_raw();
    initialize_internal_page(&mut left_page, PageId::new(70));
    for (child_page_id, separator_key) in [
        (PageId::new(10), BTreeKey::Int(20)),
        (PageId::new(30), BTreeKey::Int(40)),
        (PageId::new(50), BTreeKey::Int(60)),
    ] {
        append_internal_entry(
            &mut left_page,
            &InternalEntry::new(child_page_id, separator_key),
        )?;
    }
    let mut right_page = Page::new_raw();

    let separator = internal_split(&mut left_page, &mut right_page, BTreeKeyType::Int)?;

    let left_header =
        BTreePageHeader::read_from_page(&left_page).ok_or(InternalPageError::InvalidPage)?;
    let right_header =
        BTreePageHeader::read_from_page(&right_page).ok_or(InternalPageError::InvalidPage)?;
    assert_eq!(separator, BTreeKey::Int(40));
    assert_eq!(left_header.rightmost_child(), Some(PageId::new(30)));
    assert_eq!(right_header.rightmost_child(), Some(PageId::new(70)));
    assert_eq!(
        read_internal_entries(&left_page, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.left_child, entry.separator_key))
            .collect::<Vec<_>>(),
        vec![(PageId::new(10), BTreeKey::Int(20))]
    );
    assert_eq!(
        read_internal_entries(&right_page, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.left_child, entry.separator_key))
            .collect::<Vec<_>>(),
        vec![(PageId::new(50), BTreeKey::Int(60))]
    );
    Ok(())
}

#[test]
fn 일반_child_leaf_split_결과로_internal_page를_재구성한다()
-> Result<(), Box<dyn std::error::Error>> {
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(20)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(40)),
    )?;

    replace_child_after_leaf_split(
        &mut parent,
        BTreeKeyType::Int,
        PageId::new(20),
        PageId::new(21),
        PageId::new(22),
        BTreeKey::Int(30),
    )?;

    let header = BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
    assert_eq!(header.rightmost_child(), Some(PageId::new(30)));
    assert_eq!(
        read_internal_entries(&parent, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.left_child, entry.separator_key))
            .collect::<Vec<_>>(),
        vec![
            (PageId::new(10), BTreeKey::Int(20)),
            (PageId::new(21), BTreeKey::Int(30)),
            (PageId::new(22), BTreeKey::Int(40)),
        ]
    );
    Ok(())
}

#[test]
fn rightmost_child_leaf_split_결과로_internal_page를_재구성한다()
-> Result<(), Box<dyn std::error::Error>> {
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(20)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(40)),
    )?;

    replace_child_after_leaf_split(
        &mut parent,
        BTreeKeyType::Int,
        PageId::new(30),
        PageId::new(31),
        PageId::new(32),
        BTreeKey::Int(60),
    )?;

    let header = BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
    assert_eq!(header.rightmost_child(), Some(PageId::new(32)));
    assert_eq!(
        read_internal_entries(&parent, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.left_child, entry.separator_key))
            .collect::<Vec<_>>(),
        vec![
            (PageId::new(10), BTreeKey::Int(20)),
            (PageId::new(20), BTreeKey::Int(40)),
            (PageId::new(31), BTreeKey::Int(60)),
        ]
    );
    Ok(())
}

#[test]
fn internal_page에_공간이_없으면_child_교체를_적용하지_않는다()
-> Result<(), Box<dyn std::error::Error>> {
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Varchar("a".repeat(8160))),
    )?;
    let parent_before_replace = parent.as_bytes().to_vec();

    let result = replace_child_after_leaf_split(
        &mut parent,
        BTreeKeyType::Varchar,
        PageId::new(10),
        PageId::new(11),
        PageId::new(12),
        BTreeKey::Varchar("b".to_owned()),
    );

    assert!(matches!(result, Err(InternalPageError::PageFull)));
    assert_eq!(parent.as_bytes(), parent_before_replace);
    Ok(())
}
