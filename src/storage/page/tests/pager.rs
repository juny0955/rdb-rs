use std::{
    fs::{File, OpenOptions},
    io::{ErrorKind, Seek, SeekFrom, Write},
};

use crate::{storage::page::slot::slot_offset, test_supports::TestFile};

use super::*;

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
    let test_file = TestFile::new("allocate");

    let mut file = File::create(test_file.path()).expect("테스트 파일 생성 실패");
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

#[test]
fn allocate_손상된파일_테스트() {
    let test_file = TestFile::new("allocate-invalid");

    let mut file = File::create(test_file.path()).expect("테스트 파일 생성 실패");
    file.write_all("1".as_bytes())
        .expect("테스트 파일 작성 실패");
    let error = allocate_page(&mut file).expect_err("손상된 파일은 allocate 실패해야 한다");
    assert!(matches!(error, PagerError::InvalidFileSize));
    assert_eq!(file.metadata().expect("메타데이터 읽기 실패").len(), 1);
}

#[test]
fn read_성공_테스트() {
    let test_file = TestFile::new("read");
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let mut file = options
        .open(test_file.path())
        .expect("테스트 파일 생성 실패");
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

#[test]
fn read_eof_테스트() {
    let test_file = TestFile::new("read-eof");
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let mut file = options
        .open(test_file.path())
        .expect("테스트 파일 생성 실패");
    let _ = allocate_page(&mut file).expect("allocate 실패");

    let page_id = PageId(1);

    let error = read_page(&mut file, page_id).expect_err("Eof 에러가 반환되어야 한다");
    assert!(matches!(
        error,
        PagerError::Io(error) if error.kind() == ErrorKind::UnexpectedEof
    ));
}

#[test]
fn write_성공_테스트() {
    let test_file = TestFile::new("write");
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let mut file = options
        .open(test_file.path())
        .expect("테스트 파일 생성 실패");
    let page_id = allocate_page(&mut file).expect("allocate 실패");

    let data = [1u8; PAGE_SIZE];
    let mut page = Page::new();
    page.data = data;

    write_page(&mut file, page_id, &page).expect("write 실패");
    let page = read_page(&mut file, page_id).expect("read 실패");
    assert_eq!(page.data, data);
}

#[test]
fn write_미할당_page_id_테스트() {
    let test_file = TestFile::new("write-invalid");
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let mut file = options
        .open(test_file.path())
        .expect("테스트 파일 생성 실패");
    let _ = allocate_page(&mut file).expect("allocate 실패");
    let file_len = file.metadata().expect("metadata 읽기 실패").len();

    let data = [1u8; PAGE_SIZE];
    let mut page = Page::new();
    page.data = data;

    let error =
        write_page(&mut file, PageId(1), &page).expect_err("미할당 PageId 쓰기는 실패해야한다");
    assert!(matches!(error, PagerError::PageNotAllocated(PageId(1))));
    assert_eq!(file.metadata().expect("metadata 읽기 실패").len(), file_len);
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
    assert!(matches!(
        page_offset(PageId(u64::MAX)).expect_err("overflow 발생해야한다"),
        PagerError::PageOffsetOverflow(PageId(u64::MAX))
    ));

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
    assert!(matches!(
        slot_offset(SlotId(2046)).expect_err("page size 보다 커야한다"),
        PageError::SlotOffsetOutOfBounds
    ));
}

#[test]
fn 재시작시_page_데이터_유지_테스트() {
    let test_file = TestFile::new("restart");
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let page_id = {
        let mut file = options
            .open(test_file.path())
            .expect("테스트 파일 열기 실패");
        let page_id = allocate_page(&mut file).expect("allocate 실패");

        let page = Page::new();

        write_page(&mut file, page_id, &page).expect("write 실패");
        page_id
    };

    {
        let mut file = options
            .open(test_file.path())
            .expect("테스트 파일 열기 실패");
        let page = read_page(&mut file, page_id).expect("read 실패");
        assert_eq!(page.slot_count(), 0);
        assert_eq!(page.free_start(), HEADER_SIZE as u16);
        assert_eq!(page.free_end(), PAGE_SIZE as u16);
        assert_eq!(page.free_list_head(), u16::MAX);
    }
}
