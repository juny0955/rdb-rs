use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

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

pub struct TestRelationFile {
    directory: TempDir,
    path: PathBuf,
}

impl TestRelationFile {
    pub fn new(label: &str, file_name: &str) -> Self {
        let directory = Builder::new()
            .prefix(&format!("rdb-rs-{label}-"))
            .tempdir()
            .expect("테스트 디렉터리를 생성해야 함");
        let path = directory.path().join(file_name);
        File::create(&path).expect("relation 파일을 생성해야 함");

        Self { directory, path }
    }

    pub fn data_dir(&self) -> &Path {
        self.directory.path()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn reopen(&self) -> io::Result<File> {
        OpenOptions::new().read(true).write(true).open(&self.path)
    }
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
