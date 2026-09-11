use std::{
    collections::HashSet,
    fs::{File, OpenOptions, create_dir_all},
    io::{self},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    catalog::metadata::RelationId,
    storage::page::{
        Page, PageId, PagerError, allocate_file_page, page_count, read_page, write_page,
    },
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
    data_dir: PathBuf,
    registered_relations: HashSet<RelationId>,
}

impl RelationFileManager {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            registered_relations: HashSet::new(),
        }
    }

    pub fn register_relation(
        &mut self,
        relation_id: RelationId,
    ) -> Result<(), RelationFileManagerError> {
        open_rw(&self.relation_path(relation_id))?;
        self.registered_relations.insert(relation_id);
        Ok(())
    }

    pub fn create_relation(
        &mut self,
        relation_id: RelationId,
    ) -> Result<(), RelationFileManagerError> {
        let path = self.relation_path(relation_id);
        let _ = open_rw_create(&path)?;
        self.registered_relations.insert(relation_id);
        Ok(())
    }

    pub fn open_relation_file(
        &self,
        relation_id: RelationId,
    ) -> Result<File, RelationFileManagerError> {
        if !self.registered_relations.contains(&relation_id) {
            return Err(RelationFileManagerError::RelationNotRegistered(relation_id));
        }
        Ok(open_rw(&self.relation_path(relation_id))?)
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

    pub fn page_count(&self, relation_id: RelationId) -> Result<u64, RelationFileManagerError> {
        let file = &self.open_relation_file(relation_id)?;
        Ok(page_count(file)?)
    }

    fn relation_path(&self, relation_id: RelationId) -> PathBuf {
        match relation_id {
            RelationId::Heap(id) => self.data_dir.join(format!("{}.tbl", id.id())),
            RelationId::Index(id) => self.data_dir.join(format!("{}.idx", id.id())),
        }
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
