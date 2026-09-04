use super::*;

#[test]
fn int_internal_entry를_바이트로_직렬화한다() {
    // Given
    let entry = InternalEntry::new(PageId::new(3), BTreeKey::Int(42));

    // When
    let bytes = entry.to_bytes();

    // Then
    assert_eq!(bytes, Some(vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42]));
}

#[test]
fn int_internal_entry를_바이트에서_복원한다() {
    // Given
    let bytes = vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42];

    // When
    let entry = InternalEntry::from_bytes(BTreeKeyType::Int, &bytes);

    // Then
    assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
}
