use std::path::Path;

use tempfile::{Builder, TempDir, TempPath};

pub struct TestFile {
    path: TempPath,
}

impl TestFile {
    pub fn new(label: &str) -> Self {
        let path = Builder::new()
            .prefix(&format!("rdb-rs-{label}-"))
            .tempfile()
            .expect("테스트 파일을 생성해야 함")
            .into_temp_path();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
}

pub struct TestDirectory {
    directory: TempDir,
}

impl TestDirectory {
    pub fn new(label: &str) -> Self {
        let directory = Builder::new()
            .prefix(&format!("rdb-rs-{label}-"))
            .tempdir()
            .expect("테스트 디렉터리를 생성해야 함");
        Self { directory }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}
