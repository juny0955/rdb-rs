use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::{PageId, RowId},
};

use super::*;

#[test]
fn 빈_leaf는_underfull이다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);

    // When
    let underfull = leaf_is_underfull(&page)?;

    // Then
    assert!(underfull);
    Ok(())
}

#[test]
fn 정확히_절반을_사용한_leaf는_underfull이다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    append_leaf_entry(
        &mut page,
        &LeafEntry::new(
            BTreeKey::Varchar("x".repeat(4070)),
            RowId::new(PageId::new(1), SlotId::new(1)),
        ),
    )?;

    // When
    let underfull = leaf_is_underfull(&page)?;

    // Then
    assert!(underfull);
    Ok(())
}

#[test]
fn 절반보다_큰_leaf는_underfull이_아니다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    append_leaf_entry(
        &mut page,
        &LeafEntry::new(
            BTreeKey::Varchar("x".repeat(4071)),
            RowId::new(PageId::new(1), SlotId::new(1)),
        ),
    )?;

    // When
    let underfull = leaf_is_underfull(&page)?;

    // Then
    assert!(!underfull);
    Ok(())
}

#[test]
fn 공간이_부족하면_leaf_entry를_추가하지_않는다() {
    // Given
    let mut page = Page::new_raw();
    initialize_leaf_page(&mut page);
    let full_entry = LeafEntry::new(
        BTreeKey::Varchar("x".repeat(8166)),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );
    append_leaf_entry(&mut page, &full_entry)
        .expect("leaf page에 최대 entry를 추가할 수 있어야 한다");
    let page_before_append = page.as_bytes().to_vec();
    let entry = LeafEntry::new(
        BTreeKey::Int(42),
        RowId::new(PageId::new(4), SlotId::new(8)),
    );

    // When
    let result = append_leaf_entry(&mut page, &entry);

    // Then
    assert!(matches!(result, Err(LeafPageError::PageFull)));
    assert_eq!(page.as_bytes(), page_before_append);
    let entries = read_leaf_entries(&page, BTreeKeyType::Varchar)
        .expect("정상 leaf page의 entry를 읽을 수 있어야 한다");
    assert_eq!(
        entries
            .into_iter()
            .next()
            .and_then(|entry| entry.to_bytes()),
        full_entry.to_bytes()
    );
}
