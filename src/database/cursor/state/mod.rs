use std::vec::IntoIter;

use crate::{
    database::{
        Database, DatabaseError,
        cursor::state::{index::IndexScanState, scan::ScanState},
    },
    executor::{ExecutorError, QueryRow, select::SelectState},
    query::binder::{BoundFromClause, BoundSelect},
    storage::page::RowId,
    tuple::Value,
};

mod index;
mod scan;

pub enum InputState {
    SeqScan(ScanState),
    IndexScan(IndexScanState),
}

impl InputState {
    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        match self {
            Self::SeqScan(state) => state.next_input_row(database),
            Self::IndexScan(state) => state.next_input_row(database),
        }
    }
}

pub enum CursorState {
    Scan {
        input: InputState,
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
            input: InputState::SeqScan(ScanState::new(table.clone(), page_count)),
            state: Box::new(SelectState::new(bound)?),
        })
    }

    pub fn new_index_scan(bound: BoundSelect, row_ids: Vec<RowId>) -> Result<Self, ExecutorError> {
        let BoundFromClause::Table(table) = &bound.from else {
            return Err(ExecutorError::Unsupported);
        };

        Ok(Self::Scan {
            input: InputState::IndexScan(IndexScanState::new(table.clone(), row_ids)),
            state: Box::new(SelectState::new(bound)?),
        })
    }
}
