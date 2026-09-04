use super::*;

#[test]
fn key에_맞는_internal_child를_선택한다() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut page = Page::new_raw();
    initialize_internal_page(&mut page, PageId::new(30));
    append_internal_entry(
        &mut page,
        &InternalEntry::new(PageId::new(10), BTreeKey::Int(42)),
    )?;
    append_internal_entry(
        &mut page,
        &InternalEntry::new(PageId::new(20), BTreeKey::Int(99)),
    )?;

    // When / Then
    assert_eq!(
        find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(42))?,
        PageId::new(10)
    );
    assert_eq!(
        find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(50))?,
        PageId::new(20)
    );
    assert_eq!(
        find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(100))?,
        PageId::new(30)
    );
    Ok(())
}
