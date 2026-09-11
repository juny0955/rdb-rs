use super::*;

#[test]
fn slot_변환_테스트() {
    let org_slot = Slot {
        offset: 8000,
        length: 12,
    };
    let bytes = org_slot.to_bytes();
    let new_slot = Slot::from_bytes(bytes);

    assert_eq!(org_slot, new_slot);
    assert_eq!(bytes, [0x1F, 0x40, 0x00, 0x0C]);
}

#[test]
fn write_slot_테스트() {
    let mut page = Page::new();
    let slot = Slot {
        offset: 8000,
        length: 12,
    };
    page.write_slot(SlotId(0), &slot).expect("write slot 실패");

    assert_eq!(
        page.data[HEADER_SIZE..HEADER_SIZE + SLOT_SIZE],
        slot.to_bytes()
    );
}

#[test]
fn read_slot_테스트() {
    let mut page = Page::new();
    let slot = Slot {
        offset: 8000,
        length: 12,
    };
    let slot_index = page.add_slot(&slot).expect("add slot 실패");

    let read = page.read_slot(slot_index).expect("read slot 실패");
    assert_eq!(slot, read);
}

#[test]
fn read_slot_not_found_테스트() {
    let page = Page::new();
    let error = page
        .read_slot(SlotId(0))
        .expect_err("not found 오류 발생해야한다");
    assert!(matches!(error, PageError::SlotNotFound));
}

#[test]
fn add_slot_테스트() {
    let mut page = Page::new();
    let slot = Slot {
        offset: 8000,
        length: 12,
    };

    let slot_id = page.add_slot(&slot).expect("add slot 실패");
    assert_eq!(slot_id.0, 0);
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.free_start(), (HEADER_SIZE + SLOT_SIZE) as u16);
    assert_eq!(page.read_slot(slot_id).expect("read slot 실패"), slot);
}
