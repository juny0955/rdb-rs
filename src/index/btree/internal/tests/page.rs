use super::*;

#[test]
fn internal_page를_초기화한다() {
    // Given
    let mut page = Page::new_raw();

    // When
    initialize_internal_page(&mut page, PageId::new(42));

    // Then
    assert_eq!(
        &page.as_bytes()[0..13],
        &[1, 0, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 42]
    );
}

#[test]
fn internal_page에_entry를_추가하고_읽는다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut page = Page::new_raw();
    initialize_internal_page(&mut page, PageId::new(42));
    let entry = InternalEntry::new(PageId::new(3), BTreeKey::Int(42));
    let entry_bytes = vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42];

    // When
    append_internal_entry(&mut page, &entry)?;
    let entries = read_internal_entries(&page, BTreeKeyType::Int)?;

    // Then
    assert_eq!(
        &page.as_bytes()[0..13],
        &[1, 0, 1, 0, 27, 0, 0, 0, 0, 0, 0, 0, 42]
    );
    assert_eq!(&page.as_bytes()[13..27], entry_bytes);
    assert_eq!(
        entries
            .into_iter()
            .next()
            .and_then(|entry| entry.to_bytes()),
        Some(entry_bytes)
    );
    Ok(())
}

#[test]
fn 고정_헤더보다_짧은_internal_entry는_읽기를_거부한다() {
    // Given
    let mut page = Page::new_raw();
    initialize_internal_page(&mut page, PageId::new(42));
    page.as_bytes_mut()[1..3].copy_from_slice(&1_u16.to_be_bytes());
    page.as_bytes_mut()[3..5].copy_from_slice(&21_u16.to_be_bytes());

    // When
    let entries = read_internal_entries(&page, BTreeKeyType::Int);

    // Then
    assert!(matches!(entries, Err(InternalPageError::InvalidPage)));
}

#[test]
fn internal_page가_가득차면_entry를_추가하지_않는다() {
    let mut page = Page::new_raw();
    initialize_internal_page(&mut page, PageId::new(42));
    let entry = InternalEntry::new(PageId::new(3), BTreeKey::Varchar("x".repeat(8170)));
    let page_before_append = page.as_bytes().to_vec();

    let result = append_internal_entry(&mut page, &entry);

    assert!(matches!(result, Err(InternalPageError::PageFull)));
    assert_eq!(page.as_bytes(), page_before_append);
}
