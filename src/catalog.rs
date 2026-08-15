use std::{
    fs::{File, OpenOptions, create_dir_all},
    io::{self, Seek, SeekFrom, Write},
    path::Path,
};

use crate::schema::{DatabaseMetadata, SchemaError};

#[derive(Debug)]
pub enum CatalogError {
    Io(io::Error),
    Schema(SchemaError),
}

impl From<io::Error> for CatalogError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<SchemaError> for CatalogError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

#[derive(Debug)]
pub struct Catalog {
    file: File,
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Catalog, CatalogError> {
        let mut binding = OpenOptions::new();
        let options = binding.read(true).write(true).create(true);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_dir_all(parent)?;
        }

        let file = options.open(path)?;
        Ok(Self { file })
    }

    /// 덮어쓰기 추후 append 방식 개선
    pub fn save(&mut self, database: &DatabaseMetadata) -> Result<(), CatalogError> {
        let bytes = database.to_bytes()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }
}
