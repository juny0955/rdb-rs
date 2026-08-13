use std::{
    env::temp_dir,
    fs::{self, OpenOptions},
    path::PathBuf,
    process,
};

use super::*;

fn temp_path(path: &str) -> PathBuf {
    let temp_dir = temp_dir();
    let path = temp_dir.join(path);
    if path.exists() {
        fs::remove_file(&path).expect("테스트 정리 실패");
    }

    path
}

#[test]
fn page_id_비교_테스트() {
    let p1 = PageId(0);
    let p2 = PageId(1);

    assert_ne!(p1, p2);
}

#[test]
fn page_생성_테스트() {
    let page = Page::new();

    assert_eq!(page.data.len(), PAGE_SIZE);
    assert_eq!(page.slot_count(), 0);
    assert_eq!(page.free_start(), HEADER_SIZE as u16);
    assert_eq!(page.free_end(), PAGE_SIZE as u16);
    assert_eq!(page.free_list_head(), u16::MAX);
    assert!(page.data[HEADER_SIZE..].iter().all(|&byte| byte == 0));
}

#[test]
fn allocate_성공_테스트() {
    let path = temp_path(format!("allocate-{}.data", process::id()).as_str());

    {
        let mut file = File::create(&path).expect("테스트 파일 생성 실패");
        let page_id = allocate_page(&mut file).expect("allocate 실패");
        assert_eq!(page_id, PageId(0));
        assert_eq!(
            file.metadata().expect("메타데이터 읽기 실패").len(),
            PAGE_SIZE as u64
        );

        let page_id = allocate_page(&mut file).expect("allocate 실패");
        assert_eq!(page_id, PageId(1));
        assert_eq!(
            file.metadata().expect("메타데이터 읽기 실패").len(),
            (PAGE_SIZE * 2) as u64
        );
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn allocate_손상된파일_테스트() {
    let path = temp_path(format!("allocate-invalid-{}.data", process::id()).as_str());

    {
        let mut file = File::create(&path).expect("테스트 파일 생성 실패");
        file.write_all("1".as_bytes())
            .expect("테스트 파일 작성 실패");
        let error = allocate_page(&mut file).expect_err("손상된 파일은 allocate 실패해야 한다");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(file.metadata().expect("메타데이터 읽기 실패").len(), 1);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn read_성공_테스트() {
    let path = temp_path(format!("read-{}.data", process::id()).as_str());
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    {
        let mut file = options.open(&path).expect("테스트 파일 생성 실패");
        let p1 = allocate_page(&mut file).expect("allocate 실패");
        let p2 = allocate_page(&mut file).expect("allocate 실패");

        let d1 = &[1u8; PAGE_SIZE];
        let d2 = &[2u8; PAGE_SIZE];

        file.seek(SeekFrom::Start(0)).expect("file seek 실패");
        file.write_all(d1).expect("file write 실패");
        file.write_all(d2).expect("file write 실패");

        let page = read_page(&mut file, p1).expect("read page 실패");
        assert_eq!(page.data, *d1);

        let page = read_page(&mut file, p2).expect("read page 실패");
        assert_eq!(page.data, *d2);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn read_eof_테스트() {
    let path = temp_path(format!("read-eof-{}.data", process::id()).as_str());
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    {
        let mut file = options.open(&path).expect("테스트 파일 생성 실패");
        let _ = allocate_page(&mut file).expect("allocate 실패");

        let page_id = PageId(1);

        let error = read_page(&mut file, page_id).expect_err("Eof 에러가 반환되어야 한다");
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn write_성공_테스트() {
    let path = temp_path(format!("write-{}.data", process::id()).as_str());
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    {
        let mut file = options.open(&path).expect("테스트 파일 생성 실패");
        let page_id = allocate_page(&mut file).expect("allocate 실패");

        let data = [1u8; PAGE_SIZE];
        let mut page = Page::new();
        page.data = data;

        write_page(&mut file, page_id, &page).expect("write 실패");
        let page = read_page(&mut file, page_id).expect("read 실패");
        assert_eq!(page.data, data);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn write_미할당_page_id_테스트() {
    let path = temp_path(format!("write-invalid-{}.data", process::id()).as_str());
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    {
        let mut file = options.open(&path).expect("테스트 파일 생성 실패");
        let _ = allocate_page(&mut file).expect("allocate 실패");
        let file_len = file.metadata().expect("metadata 읽기 실패").len();

        let data = [1u8; PAGE_SIZE];
        let mut page = Page::new();
        page.data = data;

        let error =
            write_page(&mut file, PageId(1), &page).expect_err("미할당 PageId 쓰기는 실패해야한다");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert_eq!(file.metadata().expect("metadata 읽기 실패").len(), file_len);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

#[test]
fn offset_계산_테스트() {
    // page offset
    assert_eq!(page_offset(PageId(0)).expect("offset 계산 실패"), 0);
    assert_eq!(
        page_offset(PageId(1)).expect("offset 계산 실패"),
        PAGE_SIZE as u64
    );
    assert_eq!(
        page_offset(PageId(2)).expect("offset 계산 실패"),
        (PAGE_SIZE * 2) as u64
    );
    assert_eq!(
        page_offset(PageId(u64::MAX))
            .expect_err("overflow 발생해야한다")
            .kind(),
        ErrorKind::InvalidInput
    );

    // slot offset
    assert_eq!(
        slot_offset(SlotId(0)).expect("offset 계산 실패"),
        HEADER_SIZE
    );
    assert_eq!(
        slot_offset(SlotId(1)).expect("offset 계산 실패"),
        HEADER_SIZE + SLOT_SIZE
    );
    assert_eq!(
        slot_offset(SlotId(2045)).expect("offset 계산 실패"),
        HEADER_SIZE + (SLOT_SIZE * 2045)
    );
    assert_eq!(
        slot_offset(SlotId(2046))
            .expect_err("page size 보다 커야한다")
            .kind(),
        ErrorKind::InvalidInput
    );
}

#[test]
fn 재시작시_page_데이터_유지_테스트() {
    let path = temp_path(format!("restart-{}.data", process::id()).as_str());
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let page_id = {
        let mut file = options.open(&path).expect("테스트 파일 열기 실패");
        let page_id = allocate_page(&mut file).expect("allocate 실패");

        let page = Page::new();

        write_page(&mut file, page_id, &page).expect("write 실패");
        page_id
    };

    {
        let mut file = options.open(&path).expect("테스트 파일 열기 실패");
        let page = read_page(&mut file, page_id).expect("read 실패");
        assert_eq!(page.slot_count(), 0);
        assert_eq!(page.free_start(), HEADER_SIZE as u16);
        assert_eq!(page.free_end(), PAGE_SIZE as u16);
        assert_eq!(page.free_list_head(), u16::MAX);
    }

    fs::remove_file(&path).expect("테스트 정리 실패");
}

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
    assert_eq!(error.kind(), ErrorKind::NotFound);
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
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

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
    assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn update_row_테스트() {
    let mut page = Page::new();
    let row = Row::from_bytes(&[1, 2, 3]);
    let slot_id = page.insert_row(&row).expect("insert row 실패");
    let slot = page.read_slot(slot_id).expect("read slot 실패");

    let update_row = Row::from_bytes(&[4, 5, 6]);
    let _ = page
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
    assert_eq!(error.kind(), ErrorKind::NotFound);
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
