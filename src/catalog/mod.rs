use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, IndexId, IndexMetadata, SchemaError,
        TableId, TableMetadata,
    },
    storage::file::open_rw_create,
};
use thiserror::Error;

pub mod metadata;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog I/O 오류: {0}")]
    Io(#[from] io::Error),
    #[error("catalog schema 오류: {0}")]
    Schema(#[from] SchemaError),
    #[error("catalog 끝에 읽히지 않은 바이트가 남아 있습니다")]
    TrailingCatalogBytes,
}

#[derive(Debug)]
pub struct Catalog {
    file: File,
    metadata: DatabaseMetadata,
}

impl Catalog {
    pub fn open(path: &Path, database_name: &str) -> Result<Catalog, CatalogError> {
        let mut file = open_rw_create(&path.join("catalog"))?;

        match load(&mut file) {
            Ok(Some(metadata)) => Ok(Self { file, metadata }),
            Ok(None) => {
                let metadata = DatabaseMetadata::new(database_name.to_owned(), vec![], vec![])?;
                let mut catalog = Self { file, metadata };
                catalog.save()?;
                Ok(catalog)
            }
            Err(e) => Err(e),
        }
    }

    pub fn build_table_metadata(
        &self,
        name: String,
        columns: Vec<(String, DataType)>,
    ) -> Result<TableMetadata, CatalogError> {
        if self.metadata.tables().len() >= usize::from(u16::MAX) {
            return Err(CatalogError::Schema(SchemaError::TooManyTables));
        }
        if self.metadata.table(&name).is_some() {
            return Err(CatalogError::Schema(SchemaError::DuplicateTableName(name)));
        }

        let mut column_metadatas = Vec::new();
        for (index, (name, data_type)) in (1..).zip(columns) {
            let column_id = ColumnId::new(index as u32);
            column_metadatas.push(ColumnMetadata::new(column_id, name, data_type));
        }

        let table_id = TableId::new(self.metadata.tables().len() as u32 + 1);
        let table = TableMetadata::new(table_id, name, column_metadatas)?;
        Ok(table)
    }

    pub fn add_index(&mut self, index_metadata: IndexMetadata) -> Result<(), CatalogError> {
        self.metadata.add_index(index_metadata)?;
        self.save()
    }

    pub fn add_table(&mut self, table_metadata: TableMetadata) -> Result<(), CatalogError> {
        self.metadata.add_table(table_metadata)?;
        self.save()
    }

    pub fn index_column_data_type(
        &self,
        table_id: TableId,
        column_id: ColumnId,
    ) -> Result<DataType, CatalogError> {
        let table = match self.metadata.table_by_id(table_id) {
            Some(table) => table,
            None => {
                return Err(CatalogError::Schema(SchemaError::IndexTableNotFound(
                    table_id,
                )));
            }
        };

        match table.column_by_id(column_id) {
            Some(column) => Ok(column.data_type()),
            None => Err(CatalogError::Schema(SchemaError::IndexColumnNotFound {
                table_id,
                column_id,
            })),
        }
    }

    pub fn index_table(&self, table_id: TableId) -> Result<&TableMetadata, CatalogError> {
        match self.metadata.table_by_id(table_id) {
            Some(table) => Ok(table),
            None => Err(CatalogError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            ))),
        }
    }

    pub fn index_column_position(
        &self,
        table_id: TableId,
        column_id: ColumnId,
    ) -> Result<usize, CatalogError> {
        self.index_table(table_id)?
            .column_index(column_id)
            .ok_or(CatalogError::Schema(SchemaError::IndexColumnNotFound {
                table_id,
                column_id,
            }))
    }

    pub fn next_index_id(&self, index_name: &str) -> Result<IndexId, CatalogError> {
        if self.metadata.index(index_name).is_some() {
            return Err(CatalogError::Schema(SchemaError::DuplicateIndexName(
                index_name.to_owned(),
            )));
        }

        let index_len = self.metadata.indexes().len();
        if index_len >= usize::from(u16::MAX) {
            return Err(CatalogError::Schema(SchemaError::TooManyIndexes));
        }

        Ok(IndexId::new(index_len as u32 + 1))
    }

    /// TODO: 추후 append 방식 개선
    fn save(&mut self) -> Result<(), CatalogError> {
        let bytes = self.metadata.to_bytes()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }

    pub fn metadata(&self) -> &DatabaseMetadata {
        &self.metadata
    }
}

fn load(file: &mut File) -> Result<Option<DatabaseMetadata>, CatalogError> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }

    let (database, consumed) = DatabaseMetadata::from_bytes(&bytes)?;
    if consumed != bytes.len() {
        return Err(CatalogError::TrailingCatalogBytes);
    }

    Ok(Some(database))
}

#[cfg(test)]
mod catalogs {
    use crate::{catalog::metadata::DataType, test_supports::TestDirectory};

    use super::*;

    #[test]
    fn table_metadata_저장_재로딩_테스트() -> Result<(), CatalogError> {
        let directory = TestDirectory::new("catalog-reload");
        let expected = {
            let mut catalog = Catalog::open(directory.path(), "mydb")?;
            let table = catalog.build_table_metadata(
                "users".to_string(),
                vec![("name".to_string(), DataType::Varchar)],
            )?;
            catalog.add_table(table)?;
            catalog.metadata().to_bytes()?
        };
        assert!(directory.path().join("catalog").exists());

        {
            let catalog = Catalog::open(directory.path(), "ignored-on-reopen")?;
            assert_eq!(catalog.metadata().to_bytes()?, expected);
        }

        Ok(())
    }
}
