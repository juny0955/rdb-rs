use std::{
    env::temp_dir,
    fs::remove_file,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEST_FILE_ID: AtomicUsize = AtomicUsize::new(0);

pub struct TestFile {
    path: PathBuf,
}

impl TestFile {
    pub fn new(label: &str) -> Self {
        let counter = NEXT_TEST_FILE_ID.fetch_add(1, Ordering::Relaxed);
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
