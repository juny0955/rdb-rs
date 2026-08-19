use std::{
    env::temp_dir,
    fs::{create_dir, remove_dir_all, remove_file},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEST_PATH_ID: AtomicUsize = AtomicUsize::new(0);

pub struct TestFile {
    path: PathBuf,
}

impl TestFile {
    pub fn new(label: &str) -> Self {
        let counter = NEXT_TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let path = temp_dir().join(format!("rdb-rs-{label}-{}-{counter}", process::id()));
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        let _ = remove_file(&self.path);
    }
}

pub struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    pub fn new(label: &str) -> Self {
        let counter = NEXT_TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let path = temp_dir().join(format!("rdb-rs-{label}-{}-{counter}", process::id()));
        create_dir(&path).expect("테스트 디렉터리를 생성해야 함");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.path);
    }
}
