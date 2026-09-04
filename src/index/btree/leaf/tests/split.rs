use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::{PageId, RowId},
};

use super::*;

#[test]
fn leaf를_split하고_separator를반환한다() -> Result<(), Box<dyn std::error::Error>> {
    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    for (key, row_id) in [
        (
            BTreeKey::Int(10),
            RowId::new(PageId::new(1), SlotId::new(1)),
        ),
        (
            BTreeKey::Int(20),
            RowId::new(PageId::new(1), SlotId::new(2)),
        ),
        (
            BTreeKey::Int(30),
            RowId::new(PageId::new(1), SlotId::new(3)),
        ),
    ] {
        append_leaf_entry(&mut left_page, &LeafEntry::new(key, row_id))?;
    }
    let old_next = PageId::new(99);
    let mut left_header =
        BTreePageHeader::read_from_page(&left_page).expect("유효한 leaf header여야 함");
    left_header.set_next_leaf_page_id(Some(old_next));
    left_header.write_to_page(&mut left_page);
    let mut right_page = Page::new_raw();
    let right_page_id = PageId::new(2);

    let separator = leaf_split(
        &mut left_page,
        &mut right_page,
        right_page_id,
        BTreeKeyType::Int,
        LeafEntry::new(
            BTreeKey::Int(25),
            RowId::new(PageId::new(1), SlotId::new(4)),
        ),
    )?;

    let left_entries = read_leaf_entries(&left_page, BTreeKeyType::Int)?;
    let right_entries = read_leaf_entries(&right_page, BTreeKeyType::Int)?;
    let left_header =
        BTreePageHeader::read_from_page(&left_page).expect("유효한 left leaf header여야 함");
    let right_header =
        BTreePageHeader::read_from_page(&right_page).expect("유효한 right leaf header여야 함");
    assert_eq!(separator, BTreeKey::Int(20));
    assert_eq!(left_header.next_leaf_page_id(), Some(right_page_id));
    assert_eq!(right_header.next_leaf_page_id(), Some(old_next));
    assert_eq!(
        left_entries
            .into_iter()
            .map(|entry| (entry.key, entry.row_id))
            .collect::<Vec<_>>(),
        vec![
            (
                BTreeKey::Int(10),
                RowId::new(PageId::new(1), SlotId::new(1)),
            ),
            (
                BTreeKey::Int(20),
                RowId::new(PageId::new(1), SlotId::new(2)),
            ),
        ]
    );
    assert_eq!(
        right_entries
            .into_iter()
            .map(|entry| (entry.key, entry.row_id))
            .collect::<Vec<_>>(),
        vec![
            (
                BTreeKey::Int(25),
                RowId::new(PageId::new(1), SlotId::new(4)),
            ),
            (
                BTreeKey::Int(30),
                RowId::new(PageId::new(1), SlotId::new(3)),
            ),
        ]
    );
    Ok(())
}

#[test]
fn 두_leaf를_병합하면_left에_정렬된_entry를_저장하고_right를_비운다()
-> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(
            BTreeKey::Int(20),
            RowId::new(PageId::new(1), SlotId::new(2)),
        ),
    )?;
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(
            BTreeKey::Int(40),
            RowId::new(PageId::new(1), SlotId::new(4)),
        ),
    )?;
    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(
            BTreeKey::Int(10),
            RowId::new(PageId::new(1), SlotId::new(1)),
        ),
    )?;
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(
            BTreeKey::Int(30),
            RowId::new(PageId::new(1), SlotId::new(3)),
        ),
    )?;
    let old_next = PageId::new(99);
    let mut right_header =
        BTreePageHeader::read_from_page(&right_page).expect("유효한 right leaf header여야 함");
    right_header.set_next_leaf_page_id(Some(old_next));
    right_header.write_to_page(&mut right_page);

    // When
    leaf_merge(&mut left_page, &mut right_page, BTreeKeyType::Int)?;

    // Then
    assert_eq!(
        read_leaf_entries(&left_page, BTreeKeyType::Int)?
            .into_iter()
            .map(|entry| (entry.key, entry.row_id))
            .collect::<Vec<_>>(),
        vec![
            (
                BTreeKey::Int(10),
                RowId::new(PageId::new(1), SlotId::new(1))
            ),
            (
                BTreeKey::Int(20),
                RowId::new(PageId::new(1), SlotId::new(2))
            ),
            (
                BTreeKey::Int(30),
                RowId::new(PageId::new(1), SlotId::new(3))
            ),
            (
                BTreeKey::Int(40),
                RowId::new(PageId::new(1), SlotId::new(4))
            ),
        ]
    );
    assert!(read_leaf_entries(&right_page, BTreeKeyType::Int)?.is_empty());
    let left_header =
        BTreePageHeader::read_from_page(&left_page).expect("유효한 left leaf header여야 함");
    let right_header =
        BTreePageHeader::read_from_page(&right_page).expect("유효한 right leaf header여야 함");
    assert_eq!(left_header.next_leaf_page_id(), Some(old_next));
    assert_eq!(right_header.next_leaf_page_id(), None);
    Ok(())
}

#[test]
fn 병합_결과가_page에_안들어가면_두_leaf를_변경하지_않는다()
-> Result<(), Box<dyn std::error::Error>> {
    // Given
    let mut left_page = Page::new_raw();
    initialize_leaf_page(&mut left_page);
    append_leaf_entry(
        &mut left_page,
        &LeafEntry::new(
            BTreeKey::Varchar("x".repeat(8166)),
            RowId::new(PageId::new(1), SlotId::new(1)),
        ),
    )?;
    let mut right_page = Page::new_raw();
    initialize_leaf_page(&mut right_page);
    append_leaf_entry(
        &mut right_page,
        &LeafEntry::new(
            BTreeKey::Varchar("y".to_owned()),
            RowId::new(PageId::new(1), SlotId::new(2)),
        ),
    )?;
    let left_before_merge = left_page.as_bytes().to_vec();
    let right_before_merge = right_page.as_bytes().to_vec();

    // When
    let result = leaf_merge(&mut left_page, &mut right_page, BTreeKeyType::Varchar);

    // Then
    assert!(matches!(result, Err(LeafPageError::PageFull)));
    assert_eq!(left_page.as_bytes(), left_before_merge);
    assert_eq!(right_page.as_bytes(), right_before_merge);
    Ok(())
}
