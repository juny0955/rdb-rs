use std::vec::IntoIter;

use crate::{
    database::{Database, DatabaseError},
    executor::{Executor, ExecutorError, QueryRow, TableRow},
    query::binder::{BoundFromClause, BoundSelect},
    storage::page::PageId,
    tuple::Value,
};

pub enum SelectSource {
    Scan {
        next_page: u64,
        page_count: u64,
        current_rows: IntoIter<TableRow>,
        emitted: usize,
    },
    Buffered(IntoIter<Vec<Value>>),
    Done,
}

impl SelectSource {
    pub fn new_scan(page_count: u64) -> Self {
        Self::Scan {
            next_page: 0,
            page_count,
            current_rows: Vec::<TableRow>::new().into_iter(),
            emitted: 0,
        }
    }
}

pub struct SelectCursor<'a> {
    database: &'a mut Database,
    bound: BoundSelect,
    source: SelectSource,
}

impl<'a> SelectCursor<'a> {
    pub(super) fn new(
        database: &'a mut Database,
        bound: BoundSelect,
        source: SelectSource,
    ) -> Self {
        Self {
            database,
            bound,
            source,
        }
    }

    pub fn next_row(&mut self) -> Result<Option<Vec<Value>>, DatabaseError> {
        match &mut self.source {
            SelectSource::Scan {
                next_page,
                page_count,
                current_rows,
                emitted,
            } => loop {
                if let Some(limit) = self.bound.limit
                    && *emitted >= limit
                {
                    return Ok(None);
                }

                let executor = Executor::new(self.database.catalog.metadata());
                if let Some(next) = current_rows.next() {
                    let query_row = QueryRow {
                        table_rows: vec![next.clone()],
                    };

                    if let Some(filter) = &self.bound.filter {
                        if executor.row_matches_filter(&query_row, filter)? {
                            let values =
                                executor.project_row(&query_row, &self.bound.projections)?;
                            *emitted += 1;
                            return Ok(Some(values));
                        } else {
                            continue;
                        }
                    }

                    let values = executor.project_row(&query_row, &self.bound.projections)?;
                    *emitted += 1;
                    return Ok(Some(values));
                }

                if next_page >= page_count {
                    return Ok(None);
                }

                let BoundFromClause::Table(table) = &self.bound.from else {
                    return Err(ExecutorError::Unsupported.into());
                };

                let rows = self.database.table_manager.scan_page_rows(
                    &mut self.database.storage_manager,
                    table.table_id,
                    PageId::new(*next_page),
                )?;
                let table_rows = executor.decode_table_rows(table, rows)?;
                *current_rows = table_rows.into_iter();
                *next_page += 1;
            },
            SelectSource::Buffered(current) => {
                let next = current.next();
                if next.is_none() {
                    self.source = SelectSource::Done;
                }

                Ok(next)
            }
            SelectSource::Done => Ok(None),
        }
    }
}
