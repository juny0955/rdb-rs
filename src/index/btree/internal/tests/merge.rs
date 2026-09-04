use super::*;

#[test]
fn 중간_right_child를_제거하면_left_separator를_갱신한다() -> Result<(), Box<dyn std::error::Error>>
{
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(40));
    for (child, separator) in [
        (PageId::new(10), BTreeKey::Int(10)),
        (PageId::new(20), BTreeKey::Int(20)),
        (PageId::new(30), BTreeKey::Int(30)),
    ] {
        append_internal_entry(&mut parent, &InternalEntry::new(child, separator))?;
    }

    // When
    remove_right_child_after_leaf_merge(
        &mut parent,
        PageId::new(10),
        PageId::new(20),
        BTreeKeyType::Int,
    )?;

    // Then
    assert_eq!(
        read_internal_entries(&parent, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.left_child, entry.separator_key))
            .collect::<Vec<_>>(),
        vec![
            (PageId::new(10), BTreeKey::Int(20)),
            (PageId::new(30), BTreeKey::Int(30)),
        ]
    );
    assert_eq!(
        find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(20))?,
        PageId::new(10)
    );
    assert_eq!(
        find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(21))?,
        PageId::new(30)
    );
    assert_eq!(
        find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(31))?,
        PageId::new(40)
    );
    Ok(())
}

#[test]
fn rightmost_child를_제거하면_left_child가_새_rightmost가_된다()
-> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
    )?;

    // When
    remove_right_child_after_leaf_merge(
        &mut parent,
        PageId::new(20),
        PageId::new(30),
        BTreeKeyType::Int,
    )?;

    // Then
    let header = BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
    assert_eq!(header.rightmost_child(), Some(PageId::new(20)));
    assert_eq!(
        find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(10))?,
        PageId::new(10)
    );
    assert_eq!(
        find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(11))?,
        PageId::new(20)
    );
    Ok(())
}

#[test]
fn 인접하지_않은_child는_병합하지_않고_parent를_유지한다() -> Result<(), Box<dyn std::error::Error>>
{
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
    )?;
    let parent_before_merge = parent.as_bytes().to_vec();

    // When
    let result = remove_right_child_after_leaf_merge(
        &mut parent,
        PageId::new(10),
        PageId::new(30),
        BTreeKeyType::Int,
    );

    // Then
    assert!(matches!(result, Err(InternalPageError::InvalidPage)));
    assert_eq!(parent.as_bytes(), parent_before_merge);
    Ok(())
}

#[test]
fn 첫_child의_병합_pair는_오른쪽_sibling을_포함한다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
    )?;

    // When
    let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(10))?;

    // Then
    assert_eq!(pair, Some((PageId::new(10), PageId::new(20))));
    Ok(())
}

#[test]
fn rightmost_child의_병합_pair는_왼쪽_sibling을_포함한다() -> Result<(), Box<dyn std::error::Error>>
{
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(30));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
    )?;
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
    )?;

    // When
    let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(30))?;

    // Then
    assert_eq!(pair, Some((PageId::new(20), PageId::new(30))));
    Ok(())
}

#[test]
fn child가_하나인_parent는_병합_pair가_없다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(10));

    // When
    let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(10))?;

    // Then
    assert_eq!(pair, None);
    Ok(())
}

#[test]
fn parent에_없는_child는_병합_pair를_찾을수_없다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut parent = Page::new_raw();
    initialize_internal_page(&mut parent, PageId::new(20));
    append_internal_entry(
        &mut parent,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
    )?;

    // When
    let result = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(30));

    // Then
    assert!(matches!(result, Err(InternalPageError::InvalidPage)));
    Ok(())
}
