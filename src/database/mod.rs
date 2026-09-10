use std::{io, path::Path};

use crate::{
    binder::{Binder, BinderError, BoundStatement},
    catalog::{
        Catalog, CatalogError,
        metadata::{DatabaseMetadata, SchemaError},
    },
    database::{index::search_index_row_ids, table::TableCache},
    executor::ExecutorError,
    sql::ast::Statement,
    storage::{heap::HeapTableError, manager::StorageManager},
    tuple::Value,
};
use crate::{
    catalog::metadata::IndexId,
    index::btree::{BTreeKeyType, tree::BTreeError},
    storage::page::RowId,
    tuple::TupleError,
};
use thiserror::Error;

mod execute;
mod index;
mod table;

const BUFFER_POOL_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Binder(#[from] BinderError),
    #[error(transparent)]
    Executor(#[from] ExecutorError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error(transparent)]
    HeapTable(#[from] HeapTableError),
    #[error(transparent)]
    BTree(#[from] BTreeError),
    #[error(transparent)]
    Tuple(#[from] TupleError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("인덱스 엔트리를 찾을 수 없습니다: index={index_id:?}, row={row_id:?}")]
    IndexEntryNotFound { index_id: IndexId, row_id: RowId },
    #[error("INT 인덱스 키가 i32 범위를 벗어났습니다: {value}")]
    IndexKeyOutOfRange { value: i64 },
    #[error("literal과 인덱스 키 타입이 일치하지 않습니다: {key_type:?}")]
    IndexKeyTypeMismatch { key_type: BTreeKeyType },
}

#[derive(Debug)]
pub enum ExecuteResult {
    Success,
    Command { affected_rows: usize },
    Rows(Vec<Vec<Value>>),
}

pub struct Database {
    metadata: DatabaseMetadata,
    catalog: Catalog,
    storage_manager: StorageManager,
    table_cache: TableCache,
}

impl Database {
    pub fn open(data_dir: &Path, name: &str) -> Result<Self, DatabaseError> {
        let mut catalog = Catalog::open(data_dir)?;

        let metadata = match catalog.load() {
            Ok(metadata) => metadata,
            Err(CatalogError::EmptyCatalog) => {
                let metadata = DatabaseMetadata::new(name.to_owned(), vec![], vec![])?;
                catalog.save(&metadata)?;
                metadata
            }
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            metadata,
            catalog,
            storage_manager: StorageManager::new(data_dir, BUFFER_POOL_CAPACITY),
            table_cache: TableCache::new(),
        })
    }

    pub fn execute(&mut self, statement: &Statement) -> Result<ExecuteResult, DatabaseError> {
        let bound = Binder::new(&self.metadata).bind(statement)?;
        match bound {
            BoundStatement::CreateTable(b) => self.create_table(&b),
            BoundStatement::CreateIndex(b) => self.create_index(&b),
            BoundStatement::Insert(b) => self.execute_insert(&b),
            BoundStatement::Select(b) => self.execute_select(&b),
            BoundStatement::Update(b) => self.execute_update(&b),
            BoundStatement::Delete(b) => self.execute_delete(&b),
        }
    }
}

#[cfg(test)]
mod tests;
