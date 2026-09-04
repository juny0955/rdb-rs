use super::*;

#[test]
fn row_직렬화_역직렬화_테스트() {
    let bytes = [0, 1, 0x11, 0xAC, 0xFF];
    let row = Row::from_bytes(&bytes);
    assert_eq!(bytes.to_vec(), row.data);
    assert_eq!(row.to_bytes(), bytes);
}

#[test]
fn row_id_동등성_테스트() {
    let row_id = RowId(PageId(1), SlotId(1));
    assert_eq!(row_id, RowId(PageId(1), SlotId(1)));
    assert_ne!(row_id, RowId(PageId(2), SlotId(1)));
    assert_ne!(row_id, RowId(PageId(1), SlotId(2)));
}

#[test]
fn row_id를_10바이트로_직렬화한다() {
    // Given
    let row_id = RowId(PageId(3), SlotId(7));

    // When
    let bytes = row_id.to_bytes();

    // Then
    assert_eq!(bytes, [0, 0, 0, 0, 0, 0, 0, 3, 0, 7]);
}

#[test]
fn row_id를_10바이트에서_역직렬화한다() {
    // Given
    let bytes = [0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

    // When
    let row_id = RowId::from_bytes(bytes);

    // Then
    assert_eq!(row_id, RowId(PageId(3), SlotId(7)));
}

#[test]
fn insert_row_테스트() {
    let mut page = Page::new();

    let bytes = [1, 2, 3];
    let row1 = Row::from_bytes(&bytes);
    let slot_id_1 = page.insert_row(&row1).expect("insert row 실패");
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.free_start() as usize, HEADER_SIZE + SLOT_SIZE);
    assert_eq!(page.free_end() as usize, PAGE_SIZE - FREE_BLOCK_SIZE);
    assert_eq!(
        page.read_slot(slot_id_1).expect("read slot 실패"),
        Slot::new((PAGE_SIZE - FREE_BLOCK_SIZE) as u16, 3)
    );
    assert_eq!(page.read_row(slot_id_1).expect("read row 실패"), row1);

    let bytes = [4, 5];
    let row2 = Row::from_bytes(&bytes);
    let slot_id_2 = page.insert_row(&row2).expect("insert row 실패");
    assert_eq!(page.slot_count(), 2);
    assert_eq!(page.free_start() as usize, HEADER_SIZE + SLOT_SIZE * 2);
    assert_eq!(page.free_end() as usize, PAGE_SIZE - FREE_BLOCK_SIZE * 2);
    assert_eq!(
        page.read_slot(slot_id_1).expect("read slot 실패"),
        Slot::new((PAGE_SIZE - FREE_BLOCK_SIZE) as u16, 3)
    );
    assert_eq!(
        page.read_slot(slot_id_2).expect("read slot 실패"),
        Slot::new((PAGE_SIZE - FREE_BLOCK_SIZE * 2) as u16, 2)
    );
    assert_eq!(page.read_row(slot_id_1).expect("read row 실패"), row1);
    assert_eq!(page.read_row(slot_id_2).expect("read row 실패"), row2);
}

#[test]
fn read_row_테스트() {
    let mut page = Page::new();
    let bytes = [1, 2, 3];
    let row = Row::from_bytes(&bytes);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let row = page.read_row(slot_id).expect("read row 실패");

    assert_eq!(row.to_bytes(), bytes);
}

#[test]
fn read_row_not_found_테스트() {
    let page = Page::new();
    let error = page
        .read_row(SlotId(0))
        .expect_err("not found 발생해야한다");
    assert!(matches!(error, PageError::SlotNotFound));
}

#[test]
fn update_row_테스트() {
    let mut page = Page::new();
    let row = Row::from_bytes(&[1, 2, 3]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let slot = page.read_slot(slot_id).expect("read slot 실패");

    let update_row = Row::from_bytes(&[4, 5, 6]);
    page
        .update_row(slot_id, &update_row)
        .expect("update row 실패");
    assert_eq!(page.read_row(slot_id).expect("read row 실패"), update_row);
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.free_start() as usize, HEADER_SIZE + SLOT_SIZE);
    assert_eq!(page.free_end() as usize, PAGE_SIZE - FREE_BLOCK_SIZE);
    assert_eq!(slot, page.read_slot(slot_id).expect("read slot 실패"));
}

#[test]
fn delete_row_테스트() {
    let mut page = Page::new();
    let row = Row::from_bytes(&[1, 2, 3]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    assert_eq!(page.read_row(slot_id).expect("read row 실패"), row);
    let slot = page.read_slot(slot_id).expect("read slot 실패");

    page.delete_row(slot_id).expect("delete row 실패");
    let error = page
        .read_row(slot_id)
        .expect_err("not found 에러 나와야한다");
    assert!(matches!(error, PageError::SlotNotFound));
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.free_start() as usize, HEADER_SIZE + SLOT_SIZE);
    assert_eq!(page.free_end() as usize, PAGE_SIZE - FREE_BLOCK_SIZE);
    assert_eq!(page.free_list_head(), slot.offset);

    let block = page
        .read_free_block(slot.offset)
        .expect("read free block 실패");
    assert_eq!(block.next, u16::MAX);
    assert_eq!(block.length as usize, FREE_BLOCK_SIZE);
}
