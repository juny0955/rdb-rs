use std::{
    collections::HashMap,
    fs::{File, OpenOptions, create_dir_all},
    io::{self},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    catalog::metadata::RelationId,
    storage::page::{Page, PageId, PagerError, allocate_file_page, read_page, write_page},
};

#[derive(Debug, Error)]
pub enum RelationFileManagerError {
    #[error("등록되지 않은 relation 입니다: {0:?}")]
    RelationNotRegistered(RelationId),
    #[error("table file I/O 오류: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Pager(#[from] PagerError),
}

#[derive(Debug)]
pub struct RelationFileManager {
    paths: HashMap<RelationId, PathBuf>,
}

impl RelationFileManager {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }

    pub fn register_relation(&mut self, relation_id: RelationId, path: PathBuf) {
        self.paths.insert(relation_id, path);
    }

    pub fn open_relation_file(
        &self,
        relation_id: RelationId,
    ) -> Result<File, RelationFileManagerError> {
        let path = self
            .paths
            .get(&relation_id)
            .ok_or(RelationFileManagerError::RelationNotRegistered(relation_id))?;
        Ok(open_rw(path)?)
    }

    pub fn read_page(
        &self,
        relation_id: RelationId,
        page_id: PageId,
    ) -> Result<Page, RelationFileManagerError> {
        let mut file = self.open_relation_file(relation_id)?;
        Ok(read_page(&mut file, page_id)?)
    }

    pub fn write_page(
        &self,
        relation_id: RelationId,
        page_id: PageId,
        page: &Page,
    ) -> Result<(), RelationFileManagerError> {
        let mut file = self.open_relation_file(relation_id)?;
        Ok(write_page(&mut file, page_id, page)?)
    }

    pub fn allocate_page(
        &mut self,
        relation_id: RelationId,
    ) -> Result<PageId, RelationFileManagerError> {
        let mut file = self.open_relation_file(relation_id)?;
        Ok(allocate_file_page(&mut file)?)
    }
}

pub fn open_rw_create(path: &Path) -> io::Result<File> {
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_dir_all(parent)?;
    }

    let file = options.open(path)?;
    Ok(file)
}

pub fn open_rw(path: &Path) -> io::Result<File> {
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true);
    let file = options.open(path)?;
    Ok(file)
}
