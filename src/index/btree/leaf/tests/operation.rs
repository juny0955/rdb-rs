use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::{PageId, RowId},
};

use super::*;

#[test]
fn leaf_page에_첫_entry를_추가한다() {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let entry = LeafEntry::new(
        BTreeKey::Int(42),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );

    // When
    let result = append_leaf_entry(&mut page, &entry);

    // Then
    result.expect("빈 leaf page에 entry를 추가할 수 있어야 한다");
    assert_eq!(
        &page.as_bytes()[0..14],
        &[0, 0, 1, 0, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        &page.as_bytes()[14..30],
        &[0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7]
    );
}

#[test]
fn leaf_page에서_entry를_읽는다() {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let entry = LeafEntry::new(
        BTreeKey::Int(42),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );
    let appended = append_leaf_entry(&mut page, &entry);

    // When
    let entries = read_leaf_entries(&page, BTreeKeyType::Int)
        .expect("정상 leaf page의 entry를 읽을 수 있어야 한다");

    // Then
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries
            .into_iter()
            .next()
            .and_then(|entry| entry.to_bytes()),
        Some(vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7])
    );
    appended.expect("정상 leaf page에 entry를 추가할 수 있어야 한다");
}

#[test]
fn 존재하는_leaf_key를_찾는다() {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let entry = LeafEntry::new(BTreeKey::Int(42), row_id);
    append_leaf_entry(&mut page, &entry).expect("정상 leaf page에 entry를 추가할 수 있어야 한다");

    // When
    let row_ids = find_leaf_row_ids(&page, BTreeKeyType::Int, &BTreeKey::Int(42))
        .expect("정상 leaf page에서 key를 검색할 수 있어야 한다");

    // Then
    assert_eq!(row_ids, vec![row_id]);
}

#[test]
fn 존재하지_않는_leaf_key는_찾지_못한다() {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let entry = LeafEntry::new(
        BTreeKey::Int(42),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );
    append_leaf_entry(&mut page, &entry).expect("정상 leaf page에 entry를 추가할 수 있어야 한다");

    // When
    let row_ids = find_leaf_row_ids(&page, BTreeKeyType::Int, &BTreeKey::Int(7))
        .expect("정상 leaf page에서 key를 검색할 수 있어야 한다");

    // Then
    assert!(row_ids.is_empty());
}

#[test]
fn 같은_leaf_key의_모든_row_id를_찾는다() -> Result<(), LeafPageError> {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let first_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let second_row_id = RowId::new(PageId::new(4), SlotId::new(8));
    append_leaf_entry(&mut page, &LeafEntry::new(BTreeKey::Int(42), first_row_id))?;
    append_leaf_entry(
        &mut page,
        &LeafEntry::new(BTreeKey::Int(7), RowId::new(PageId::new(5), SlotId::new(9))),
    )?;
    append_leaf_entry(&mut page, &LeafEntry::new(BTreeKey::Int(42), second_row_id))?;

    // When
    let row_ids = find_leaf_row_ids(&page, BTreeKeyType::Int, &BTreeKey::Int(42))?;

    // Then
    assert_eq!(row_ids, vec![first_row_id, second_row_id]);
    Ok(())
}

#[test]
fn leaf_entry_하나를_삭제하고_나머지를_유지한다() -> Result<(), Box<dyn std::error::Error>> {
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let deleted_row_id = RowId::new(PageId::new(3), SlotId::new(7));
    let remaining_row_id = RowId::new(PageId::new(4), SlotId::new(8));
    append_leaf_entry(
        &mut page,
        &LeafEntry::new(BTreeKey::Int(10), deleted_row_id),
    )?;
    append_leaf_entry(
        &mut page,
        &LeafEntry::new(BTreeKey::Int(20), remaining_row_id),
    )?;

    assert!(delete_leaf_entry(
        &mut page,
        BTreeKeyType::Int,
        &BTreeKey::Int(10),
        deleted_row_id,
    )?);
    assert_eq!(
        read_leaf_entries(&page, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.key, entry.row_id))
            .collect::<Vec<_>>(),
        vec![(BTreeKey::Int(20), remaining_row_id)]
    );
    Ok(())
}

#[test]
fn 삭제할_leaf_entry가_없으면_page를_변경하지_않는다() -> Result<(), Box<dyn std::error::Error>> {
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let row_id = RowId::new(PageId::new(3), SlotId::new(7));
    append_leaf_entry(&mut page, &LeafEntry::new(BTreeKey::Int(10), row_id))?;
    let page_before_delete = page.as_bytes().to_vec();

    assert!(!delete_leaf_entry(
        &mut page,
        BTreeKeyType::Int,
        &BTreeKey::Int(10),
        RowId::new(PageId::new(3), SlotId::new(8)),
    )?);
    assert_eq!(page.as_bytes(), page_before_delete);
    Ok(())
}
