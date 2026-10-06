use std::vec::IntoIter;

use crate::{
    database::{Database, DatabaseError},
    executor::{Executor, ExecutorError, QueryRow, TableRow, aggregate::AggregateState},
    query::binder::{BoundFromClause, BoundProjection, BoundSelect},
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
    AggregateScan {
        next_page: u64,
        page_count: u64,
        current_rows: IntoIter<TableRow>,
        emitted: bool,
        states: Vec<AggregateState>,
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

    pub fn new_aggregate(
        page_count: u64,
        projections: &[BoundProjection],
    ) -> Result<Self, ExecutorError> {
        let mut states = Vec::new();
        for projection in projections {
            match projection {
                BoundProjection::Aggregate(aggregate) => {
                    states.push(AggregateState::new(aggregate))
                }
                _ => return Err(ExecutorError::Unsupported),
            }
        }

        Ok(Self::AggregateScan {
            next_page: 0,
            page_count,
            current_rows: Vec::<TableRow>::new().into_iter(),
            emitted: false,
            states,
        })
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

    pub(super) fn new_aggregate(
        database: &'a mut Database,
        bound: BoundSelect,
        page_count: u64,
    ) -> Result<Self, DatabaseError> {
        let source = SelectSource::new_aggregate(page_count, &bound.projections)?;
        Ok(Self {
            database,
            bound,
            source,
        })
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
            SelectSource::AggregateScan {
                next_page,
                page_count,
                current_rows,
                emitted,
                states,
            } => {
                if let Some(limit) = self.bound.limit
                    && limit == 0
                {
                    return Ok(None);
                }

                if *emitted {
                    return Ok(None);
                }

                let executor = Executor::new(self.database.catalog.metadata());
                loop {
                    for next in current_rows.by_ref() {
                        let query_row = QueryRow::new(vec![next]);

                        if let Some(filter) = &self.bound.filter
                            && !executor.row_matches_filter(&query_row, filter)?
                        {
                            continue;
                        }

                        for (state, projection) in states.iter_mut().zip(&self.bound.projections) {
                            executor.accumulate_aggregate(state, projection, &query_row)?;
                        }
                    }

                    if next_page >= page_count {
                        break;
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
                }
                *emitted = true;

                let mut results = Vec::new();
                for state in states {
                    results.push(state.result()?);
                }

                Ok(Some(results))
            }
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
