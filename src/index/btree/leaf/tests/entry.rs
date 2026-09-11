use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::{PageId, RowId},
};

use super::*;

#[test]
fn int_leaf_entry를_바이트로_직렬화한다() {
    // Given
    let entry = LeafEntry::new(
        BTreeKey::Int(42),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );

    // When
    let bytes = entry.to_bytes();

    // Then
    assert_eq!(
        bytes,
        Some(vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7,])
    );
}

#[test]
fn varchar_leaf_entry를_바이트로_직렬화한다() {
    // Given
    let entry = LeafEntry::new(
        BTreeKey::Varchar(String::from("hi")),
        RowId::new(PageId::new(3), SlotId::new(7)),
    );

    // When
    let bytes = entry.to_bytes();

    // Then
    assert_eq!(
        bytes,
        Some(vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7])
    );
}

#[test]
fn int_leaf_entry를_바이트에서_복원한다() {
    // Given
    let bytes = vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

    // When
    let entry = LeafEntry::from_bytes(BTreeKeyType::Int, &bytes);

    // Then
    assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
}

#[test]
fn varchar_leaf_entry를_바이트에서_복원한다() {
    // Given
    let bytes = vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

    // When
    let entry = LeafEntry::from_bytes(BTreeKeyType::Varchar, &bytes);

    // Then
    assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
}

#[test]
fn 잘못된_boolean_바이트를_거부한다() {
    // Given
    let bytes = [0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

    // When
    let entry = LeafEntry::from_bytes(BTreeKeyType::Boolean, &bytes);

    // Then
    assert!(entry.is_none());
}
