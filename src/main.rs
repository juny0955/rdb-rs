use std::{
    fs::{File, OpenOptions, create_dir_all},
    io::Result,
    path::Path,
};

const PAGE_SIZE: usize = 8192;

#[derive(Debug, PartialEq, Eq)]
struct PageId(u64);

struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    fn new() -> Self {
        Self {
            data: [0u8; PAGE_SIZE],
        }
    }
}

fn main() -> Result<()> {
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);

    let _file = file_open(options, Path::new("data/table.data"))?;

    Ok(())
}

fn file_open(options: &mut OpenOptions, path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    options.open(path)
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{self, OpenOptions},
        process,
    };

    use crate::{PAGE_SIZE, Page, PageId, file_open};

    fn options() -> OpenOptions {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        options
    }

    #[test]
    fn 파일_생성_오픈_테스트() {
        let temp_dir = temp_dir();
        let path = temp_dir.join(format!("test-{}.data", process::id()));

        if path.exists() {
            fs::remove_file(&path).expect("테스트 정리 실패");
        }
        assert!(!path.exists());

        {
            let mut options = options();
            let _ = file_open(&mut options, &path).expect("정상 생성되어야한다.");
            assert!(path.exists());
        }

        {
            let mut options = options();
            let _ = file_open(&mut options, &path).expect("정상 오픈되어야한다.");
            assert!(path.exists());
        }

        fs::remove_file(&path).expect("테스트 정리 실패");
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
}
