use std::{
    fs::File,
    io::{Error, ErrorKind, Result, Seek, SeekFrom, Write},
};

const PAGE_SIZE: usize = 8192;

#[derive(Debug, PartialEq, Eq)]
pub struct PageId(u64);

pub struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new() -> Self {
        Self {
            data: [0; PAGE_SIZE],
        }
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

#[cfg(test)]
mod tests {
    use std::{env::temp_dir, fs, path::PathBuf, process};

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
        assert!(page.data.iter().all(|&byte| byte == 0));
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
}
