use crate::{
    database::{Database, DatabaseError},
    executor::{QueryRow, decode_table_rows},
    query::binder::BoundTable,
    storage::page::RowId,
};

pub struct IndexScanState {
    table: BoundTable,
    row_ids: Vec<RowId>,
    next_row: usize,
}

impl IndexScanState {
    pub fn new(table: BoundTable, row_ids: Vec<RowId>) -> Self {
        Self {
            table,
            row_ids,
            next_row: 0,
        }
    }

    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        if let Some(row_id) = self.next_row_id() {
            let rows = database.table_manager.get_rows(
                &mut database.storage_manager,
                self.table.table_id,
                vec![row_id],
            )?;

            let table_rows = decode_table_rows(database.catalog.metadata(), &self.table, rows)?;

            return Ok(Some(QueryRow::new(table_rows)));
        }

        Ok(None)
    }

    pub fn rescan(&mut self) {
        self.next_row = 0;
    }

    fn next_row_id(&mut self) -> Option<RowId> {
        let row_id = self.row_ids.get(self.next_row).copied()?;
        self.next_row += 1;
        Some(row_id)
    }

    #[cfg(test)]
    pub(super) fn remaining_row_ids(&self) -> usize {
        self.row_ids.len() - self.next_row
    }
}
