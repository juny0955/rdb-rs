use std::vec::IntoIter;

use crate::{
    database::{Database, DatabaseError},
    executor::{Executor, QueryRow, TableRow},
    query::binder::BoundTable,
    storage::page::PageId,
};

pub struct ScanState {
    table: BoundTable,
    next_page: u64,
    page_count: u64,
    current_rows: IntoIter<TableRow>,
}

impl ScanState {
    #[cfg(test)]
    pub(super) fn page_progress(&self) -> (u64, u64) {
        (self.next_page, self.page_count)
    }

    pub fn new(table: BoundTable, page_count: u64) -> Self {
        Self {
            table,
            page_count,
            next_page: 0,
            current_rows: Vec::<TableRow>::new().into_iter(),
        }
    }

    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        loop {
            if let Some(next) = self.current_rows.next() {
                return Ok(Some(QueryRow::new(vec![next])));
            }

            if self.next_page >= self.page_count {
                return Ok(None);
            }

            let rows = database.table_manager.scan_page_rows(
                &mut database.storage_manager,
                self.table.table_id,
                PageId::new(self.next_page),
            )?;

            let executor = Executor::new(database.catalog.metadata());
            let table_rows = executor.decode_table_rows(&self.table, rows)?;
            self.current_rows = table_rows.into_iter();
            self.next_page += 1;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/select_cursor_scan.rs"]
mod tests;
