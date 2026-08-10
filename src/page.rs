use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write},
};

const PAGE_SIZE: usize = 8192;
const SLOT_COUNT_OFFSET: usize = 0;
const FREE_START_OFFSET: usize = 2;
const FREE_END_OFFSET: usize = 4;
const HEADER_SIZE: usize = 6;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PageId(u64);

#[derive(Debug)]
pub struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new() -> Self {
        let mut data = [0u8; PAGE_SIZE];
        data[FREE_START_OFFSET..FREE_END_OFFSET]
            .copy_from_slice(&(HEADER_SIZE as u16).to_be_bytes());
        data[FREE_END_OFFSET..HEADER_SIZE].copy_from_slice(&(PAGE_SIZE as u16).to_be_bytes());

        Self { data }
    }

    pub fn slot_count(&self) -> u16 {
        u16::from_be_bytes([
            self.data[SLOT_COUNT_OFFSET],
            self.data[FREE_START_OFFSET - 1],
        ])
    }

    pub fn set_slot_count(&mut self, value: u16) {
        self.data[SLOT_COUNT_OFFSET..FREE_START_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub fn free_start(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_START_OFFSET], self.data[FREE_END_OFFSET - 1]])
    }

    pub fn set_free_start(&mut self, value: u16) {
        self.data[FREE_START_OFFSET..FREE_END_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub fn free_end(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_END_OFFSET], self.data[HEADER_SIZE - 1]])
    }

    pub fn set_free_end(&mut self, value: u16) {
        self.data[FREE_END_OFFSET..HEADER_SIZE].copy_from_slice(&value.to_be_bytes());
    }
}

/// Database File 끝에 0으로 초기화된 8KB(PAGE_SIZE) Page를 추가하고 PageId 반환
pub fn allocate_page(file: &mut File) -> Result<PageId> {
    let file_len = file.metadata()?.len();
    if file_len % PAGE_SIZE as u64 != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "invalid database file size",
        ));
    }

    let current_page = file_len / PAGE_SIZE as u64;
    file.seek(SeekFrom::End(0))?;
    file.write_all(&Page::new().data)?;
    Ok(PageId(current_page))
}

/// Database File를 PageId의 Offset을 계산하여 8KB(PAGE_SIZE)만큼 읽는다
pub fn read_page(file: &mut File, page_id: PageId) -> Result<Page> {
    let offset = page_offset(page_id)?;
    let mut page = Page::new();

    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut page.data)?;
    Ok(page)
}

/// Database File에 PageId의 Offset을 계산하여 Page 데이터를 덮어쓴다
pub fn write_page(file: &mut File, page_id: PageId, page: &Page) -> Result<()> {
    let file_len = file.metadata()?.len();
    if file_len % PAGE_SIZE as u64 != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "invalid database file size",
        ));
    }

    if page_id.0 >= file_len / PAGE_SIZE as u64 {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid page id"));
    }

    let offset = page_offset(page_id)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&page.data)?;
    Ok(())
}

fn page_offset(page_id: PageId) -> Result<u64> {
    let offset = page_id
        .0
        .checked_mul(PAGE_SIZE as u64)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "page offset overflow"))?;

    Ok(offset)
}

#[cfg(test)]
mod tests {
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

            let error = write_page(&mut file, PageId(1), &page)
                .expect_err("미할당 PageId 쓰기는 실패해야한다");
            assert_eq!(error.kind(), ErrorKind::InvalidInput);
            assert_eq!(file.metadata().expect("metadata 읽기 실패").len(), file_len);
        }

        fs::remove_file(&path).expect("테스트 정리 실패");
    }

    #[test]
    fn offset_계산_테스트() {
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
        }

        fs::remove_file(&path).expect("테스트 정리 실패");
    }
}
