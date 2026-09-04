use super::*;

#[test]
fn update_row는_더_짧아진_공간을_free_block으로_반환한다() {
    let mut page = Page::new();
    let row = Row::from_bytes(&[1; 8]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let slot = page.read_slot(slot_id).expect("read slot 실패");
    let update_row = Row::from_bytes(&[2; 4]);

    page.update_row(slot_id, &update_row)
        .expect("더 짧은 Row update 실패");

    assert_eq!(page.read_row(slot_id).expect("read row 실패"), update_row);
    assert_eq!(
        page.read_slot(slot_id).expect("read slot 실패").offset,
        slot.offset
    );

    let free_offset = slot.offset + 4;
    let free_block = page
        .read_free_block(free_offset)
        .expect("반환한 free block을 읽어야 함");
    assert_eq!(page.free_list_head(), free_offset);
    assert_eq!(free_block.length, 4);
}

#[test]
fn update_row는_더_긴_row를_새_공간으로_이동하고_slot_id를_유지한다() {
    let mut page = Page::new();
    let row = Row::from_bytes(&[1; 4]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let old_slot = page.read_slot(slot_id).expect("read slot 실패");
    let update_row = Row::from_bytes(&[2; 8]);

    page.update_row(slot_id, &update_row)
        .expect("더 긴 Row update 실패");

    assert_eq!(page.read_row(slot_id).expect("read row 실패"), update_row);
    let updated_slot = page.read_slot(slot_id).expect("read slot 실패");
    assert_ne!(updated_slot.offset, old_slot.offset);

    let free_block = page
        .read_free_block(old_slot.offset)
        .expect("이전 Row 영역은 free block이어야 함");
    assert_eq!(page.free_list_head(), old_slot.offset);
    assert_eq!(free_block.length, 4);
}

#[test]
fn update_row는_공간_부족시_압축후_재시도한다() {
    let mut page = Page::new();
    let deleted_row = Row::from_bytes(&vec![1; 4000]);
    let target_row = Row::from_bytes(&vec![2; 4000]);
    let deleted_slot = page.insert_row(&deleted_row).expect("첫 Row insert 실패");
    let target_slot = page.insert_row(&target_row).expect("둘째 Row insert 실패");
    page.delete_row(deleted_slot).expect("첫 Row delete 실패");

    let update_row = Row::from_bytes(&vec![3; 4100]);
    page.update_row(target_slot, &update_row)
        .expect("압축 후 더 긴 Row update 실패");

    assert_eq!(
        page.read_row(target_slot).expect("수정된 Row를 읽어야 함"),
        update_row
    );
    assert!(matches!(
        page.read_row(deleted_slot)
            .expect_err("삭제된 Row는 계속 읽을 수 없어야 함"),
        PageError::SlotNotFound
    ));
}

#[test]
fn compact_테스트() {
    let mut page = Page::new();
    let row1 = Row::from_bytes(&[1, 2, 3]);
    let row2 = Row::from_bytes(&[4, 5, 6]);
    let row3 = Row::from_bytes(&[7, 8, 9]);
    let slot1 = page.insert_row(&row1).expect("insert row 실패");
    let slot2 = page.insert_row(&row2).expect("insert row 실패");
    let slot3 = page.insert_row(&row3).expect("insert row 실패");
    let free_end = page.free_end();

    page.delete_row(slot2).expect("delete row 실패");
    page.compact().expect("compact 실패");
    assert_eq!(page.read_row(slot1).expect("read row 실패"), row1);
    assert_eq!(page.read_row(slot3).expect("read row 실패"), row3);
    assert_eq!(page.free_list_head(), u16::MAX);
    assert!(page.free_end() > free_end);
    assert!(matches!(
        page.read_row(slot2).expect_err("not found 오류 반환해야함"),
        PageError::SlotNotFound
    ));
}

#[test]
fn insert_row_compress_테스트() {
    let mut page = Page::new();
    let row = Row::from_bytes(&vec![1; 1020]);
    let mut slots = Vec::new();

    for _ in 0..7 {
        slots.push(page.insert_row(&row).expect("insert row 실패"));
    }

    let last_row = Row::from_bytes(&vec![2; 1012]);
    slots.push(page.insert_row(&last_row).expect("inser row 실패"));
    page.delete_row(slots[3]).expect("delete row 실패");

    let inserted = Row::from_bytes(&vec![3; 1016]);
    let new_slot = page.insert_row(&inserted).expect("insert row 실패");

    for i in 0..7 {
        if i == 3 {
            assert!(matches!(
                page.read_row(slots[i])
                    .expect_err("not found 오류 반환해야한다"),
                PageError::SlotNotFound
            ));
            continue;
        }

        assert_eq!(row, page.read_row(slots[i]).expect("read row 실패"));
    }
    assert_eq!(page.read_row(slots[7]).expect("read row 실패"), last_row);
    assert_eq!(page.read_row(new_slot).expect("read row 실패"), inserted);
    assert_eq!(page.free_list_head(), u16::MAX);
}
