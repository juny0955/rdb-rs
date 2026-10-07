use std::{io, path::Path};

use crate::database::select_cursor::SelectCursor;
use crate::index::{IndexError, IndexManager};
use crate::query::binder::{BoundCreateIndex, BoundCreateTable};
use crate::storage::{StorageError, StorageManager};
use crate::table::{TableError, TableManager};
use crate::tuple::TupleError;
use crate::{
    catalog::{Catalog, CatalogError},
    executor::ExecutorError,
    query::binder::{Binder, BinderError, BoundStatement},
    query::sql::ast::Statement,
};
use thiserror::Error;

mod execute;
mod select_cursor;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Binder(#[from] BinderError),
    #[error(transparent)]
    Executor(#[from] ExecutorError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Table(#[from] TableError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Tuple(#[from] TupleError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub enum ExecuteResult<'a> {
    Success,
    AffectedRows { affected_rows: usize },
    Rows(Box<SelectCursor<'a>>),
}

pub struct Database {
    catalog: Catalog,
    storage_manager: StorageManager,
    table_manager: TableManager,
}

impl Database {
    pub fn open(data_dir: &Path, name: &str) -> Result<Self, DatabaseError> {
        let catalog = Catalog::open(data_dir, name)?;

        Ok(Self {
            catalog,
            storage_manager: StorageManager::new(data_dir),
            table_manager: TableManager::new(),
        })
    }

    pub fn execute(&mut self, statement: &Statement) -> Result<ExecuteResult<'_>, DatabaseError> {
        let bound = Binder::new(self.catalog.metadata()).bind(statement)?;
        match bound {
            BoundStatement::CreateTable(b) => self.create_table(b),
            BoundStatement::CreateIndex(b) => self.create_index(b),
            BoundStatement::Insert(b) => self.execute_insert(b),
            BoundStatement::Select(b) => self.execute_select(b),
            BoundStatement::Update(b) => self.execute_update(b),
            BoundStatement::Delete(b) => self.execute_delete(b),
        }
    }

    fn create_index(
        &mut self,
        bound: BoundCreateIndex,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        IndexManager::create_index(
            &mut self.storage_manager,
            &mut self.catalog,
            &mut self.table_manager,
            &bound.index_name,
            bound.table_id,
            bound.column_id,
        )?;
        Ok(ExecuteResult::Success)
    }

    fn create_table(
        &mut self,
        bound: BoundCreateTable,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        let columns = bound
            .columns
            .iter()
            .map(|column| (column.name.to_owned(), column.data_type))
            .collect();
        let table = self
            .catalog
            .build_table_metadata(bound.table.to_owned(), columns)?;

        self.table_manager
            .create_table(&mut self.storage_manager, table.id())?;
        self.catalog.add_table(table)?;

        Ok(ExecuteResult::Success)
    }
}

#[cfg(test)]
mod tests;
