use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::{
    schema::{DatabaseMetadata, SchemaError},
    storage::file::open_rw_create,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog I/O 오류: {0}")]
    Io(#[from] io::Error),
    #[error("catalog schema 오류: {0}")]
    Schema(#[from] SchemaError),
    #[error("catalog이 비어 있습니다")]
    EmptyCatalog,
    #[error("catalog 끝에 읽히지 않은 바이트가 남아 있습니다")]
    TrailingCatalogBytes,
}

#[derive(Debug)]
pub struct Catalog {
    file: File,
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Catalog, CatalogError> {
        let file = open_rw_create(path)?;
        Ok(Self { file })
    }

    /// TODO: 추후 append 방식 개선
    pub fn save(&mut self, database: &DatabaseMetadata) -> Result<(), CatalogError> {
        let bytes = database.to_bytes()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }

    pub fn load(&mut self) -> Result<DatabaseMetadata, CatalogError> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        if bytes.is_empty() {
            return Err(CatalogError::EmptyCatalog);
        }

        let (database, consumed) = DatabaseMetadata::from_bytes(&bytes)?;
        if consumed != bytes.len() {
            return Err(CatalogError::TrailingCatalogBytes);
        }

        Ok(database)
    }
}

#[cfg(test)]
mod catalogs {
    use crate::{
        schema::{ColumnId, ColumnMetadata, DataType, TableId, TableMetadata},
        test_supports::TestFile,
    };

    use super::*;

    #[test]
    fn 저장_재로딩_테스트() -> Result<(), CatalogError> {
        let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
        let table = TableMetadata::new(TableId::new(1), "users".to_string(), vec![column]).unwrap();
        let database = DatabaseMetadata::new("mydb".to_string(), vec![table], vec![])?;

        let test_file = TestFile::new("catalog-reload");
        {
            let mut catalog = Catalog::open(test_file.path())?;
            catalog.save(&database)?;
        }

        {
            let mut catalog = Catalog::open(test_file.path())?;
            let load = catalog.load()?;
            assert_eq!(database, load);
        }

        Ok(())
    }
}
