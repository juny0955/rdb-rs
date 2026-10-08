use std::vec::IntoIter;

use crate::{
    database::{Database, DatabaseError},
    executor::{Executor, QueryRow},
    query::binder::BoundTable,
    storage::page::RowId,
};

pub struct IndexScanState {
    table: BoundTable,
    row_ids: IntoIter<RowId>,
}

impl IndexScanState {
    pub fn new(table: BoundTable, row_ids: Vec<RowId>) -> Self {
        Self {
            table,
            row_ids: row_ids.into_iter(),
        }
    }

    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        if let Some(row_id) = self.row_ids.next() {
            let rows = database.table_manager.get_rows(
                &mut database.storage_manager,
                self.table.table_id,
                vec![row_id],
            )?;
            let executor = Executor::new(database.catalog.metadata());
            let table_rows = executor.decode_table_rows(&self.table, rows)?;

            return Ok(Some(QueryRow::new(table_rows)));
        }

        Ok(None)
    }
}
