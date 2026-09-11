use crate::storage::page::free_space::FreeBlock;

use super::*;

#[test]
fn free_space_테스트() {
    let mut page = Page::new();
    assert_eq!(
        page.free_space().expect("free space 계산 실패"),
        PAGE_SIZE - HEADER_SIZE
    );

    page.add_slot(&Slot {
        offset: 8000,
        length: 12,
    })
    .expect("slot 추가 실패");
    assert_eq!(
        page.free_space().expect("free space 계산 실패"),
        PAGE_SIZE - HEADER_SIZE - SLOT_SIZE
    );

    page.set_free_end(1);
    let error = page.free_space().expect_err("손상된 page 이어야한다");
    assert!(matches!(error, PageError::InvalidFreeSpaceBounds));
}

#[test]
fn insert_row_free_block_size_over_테스트() {
    let mut page = Page::new();

    let bytes = [1, 2, 3, 4, 5, 6];
    let row1 = Row::from_bytes(&bytes);
    let slot_id1 = page.insert_row(&row1).expect("insert row 실패");
    let slot1 = page.read_slot(slot_id1).expect("read slot 실패");
    page.delete_row(slot_id1).expect("delete row 실패");
    let free_end = page.free_end();

    let bytes = [1, 2, 3];
    let row2 = Row::from_bytes(&bytes);
    let slot_id2 = page.insert_row(&row2).expect("insert row 실패");
    let new_row = page.read_row(slot_id2).expect("read row 실패");
    let free_head = page
        .read_free_block(page.free_list_head())
        .expect("read block 실패");
    assert_eq!(row2, new_row);
    assert_eq!(page.free_end(), free_end);
    assert_eq!(page.free_list_head(), slot1.offset + 4);
    assert_eq!(free_head.length, 4);
    assert_eq!(free_head.next, u16::MAX);
}

#[test]
fn insert_row_free_block_size_equals_테스트() {
    let mut page = Page::new();

    let bytes = [1, 2, 3];
    let row = Row::from_bytes(&bytes);
    let slot_id1 = page.insert_row(&row).expect("insert row 실패");
    page.delete_row(slot_id1).expect("delete row 실패");
    let free_end = page.free_end();

    let slot_id2 = page.insert_row(&row).expect("insert row 실패");
    let new_row = page.read_row(slot_id2).expect("read row 실패");
    assert_eq!(row, new_row);
    assert_eq!(page.free_end(), free_end);
    assert_eq!(page.free_list_head(), u16::MAX);
}

#[test]
fn free_block_직렬화_역직렬화_테스트() {
    let bytes = [0xFF, 0xFF, 0x00, 0x0C];
    let free_block = FreeBlock::from_bytes(bytes);
    assert_eq!(free_block.next, u16::MAX);
    assert_eq!(free_block.length, 12);
    assert_eq!(free_block.to_bytes(), bytes);
}

#[test]
fn write_free_block_테스트() {
    let mut page = Page::new();
    page.set_free_end(1000);
    let offset = 1000;

    let block = FreeBlock {
        next: u16::MAX,
        length: 100,
    };

    page.write_free_block(offset, &block)
        .expect("write free block 실패");
    assert_eq!(
        &page.data[offset as usize..offset as usize + FREE_BLOCK_SIZE],
        &block.to_bytes()
    );
}

#[test]
fn read_free_block_테스트() {
    let mut page = Page::new();
    page.set_free_end(1000);
    let offset = 1000;

    let block = FreeBlock {
        next: u16::MAX,
        length: 100,
    };

    page.write_free_block(offset, &block)
        .expect("write free block 실패");
    let read = page.read_free_block(offset).expect("read free block 실패");
    assert_eq!(read, block);
}

#[test]
fn row_allocation_size_테스트() {
    assert_eq!(row_allocation_size(0), 4);
    assert_eq!(row_allocation_size(3), 4);
    assert_eq!(row_allocation_size(4), 4);
    assert_eq!(row_allocation_size(6), 8);
}

#[test]
fn find_free_block_테스트() {
    let mut page = Page::new();
    assert_eq!(page.find_free_block(4).expect("find free block 실패"), None);

    let row = Row::from_bytes(&[1, 2, 3]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let slot = page.read_slot(slot_id).expect("read slot 실패");
    page.delete_row(slot_id).expect("delete row 실패");

    let (offset, prev, block) = page
        .find_free_block(4)
        .expect("find free block 실패")
        .unwrap();
    assert_eq!(offset, slot.offset);
    assert_eq!(prev, None);
    assert_eq!(block.length, 4);

    assert_eq!(page.find_free_block(8).expect("find free block 실패"), None);
}
