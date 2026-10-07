use std::vec::IntoIter;

use crate::{
    database::{Database, DatabaseError},
    executor::{
        Executor, ExecutorError, QueryRow, TableRow,
        select::{SelectState, SelectStep},
    },
    query::binder::{BoundFromClause, BoundSelect, BoundTable},
    storage::page::PageId,
    tuple::Value,
};

pub struct ScanState {
    table: BoundTable,
    next_page: u64,
    page_count: u64,
    current_rows: IntoIter<TableRow>,
}

impl ScanState {
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

pub enum CursorState {
    Scan {
        scan: ScanState,
        state: Box<SelectState>,
    },
    Buffered(IntoIter<Vec<Value>>),
    Done,
}

impl CursorState {
    pub fn new_scan(bound: BoundSelect, page_count: u64) -> Result<Self, ExecutorError> {
        let BoundFromClause::Table(table) = &bound.from else {
            return Err(ExecutorError::Unsupported);
        };

        Ok(Self::Scan {
            scan: ScanState::new(table.clone(), page_count),
            state: Box::new(SelectState::new(bound)?),
        })
    }
}

pub struct SelectCursor<'a> {
    database: &'a mut Database,
    state: CursorState,
}

impl<'a> SelectCursor<'a> {
    pub(super) fn new(database: &'a mut Database, state: CursorState) -> Self {
        Self { database, state }
    }

    pub fn next_row(&mut self) -> Result<Option<Vec<Value>>, DatabaseError> {
        match &mut self.state {
            CursorState::Scan { scan, state } => loop {
                if state.is_done() {
                    return Ok(None);
                }

                let step = if let Some(row) = scan.next_input_row(self.database)? {
                    let executor = Executor::new(self.database.catalog.metadata());
                    state.push_row(&executor, &row)?
                } else {
                    state.finish()?
                };

                return match step {
                    SelectStep::Row(values) => Ok(Some(values)),
                    SelectStep::NeedInput => continue,
                    SelectStep::Done => Ok(None),
                };
            },
            CursorState::Buffered(current) => {
                let next = current.next();
                if next.is_none() {
                    self.state = CursorState::Done;
                }

                Ok(next)
            }
            CursorState::Done => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "tests/select_cursor_scan.rs"]
mod tests;
