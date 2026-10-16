use std::vec::IntoIter;

use crate::{
    database::{
        Database, DatabaseError,
        cursor::state::{index::IndexScanState, join::JoinScanState, scan::ScanState},
    },
    executor::{ExecutorError, QueryRow, select::SelectState},
    query::binder::{BoundFromClause, BoundSelect, TableInstanceId},
    storage::page::RowId,
    tuple::Value,
};

mod index;
mod join;
mod scan;

pub enum InputState {
    SeqScan(ScanState),
    IndexScan(IndexScanState),
    HashJoin(JoinScanState),
}

impl InputState {
    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        match self {
            Self::SeqScan(state) => state.next_input_row(database),
            Self::IndexScan(state) => state.next_input_row(database),
            Self::HashJoin(state) => state.next_input_row(database),
        }
    }

    pub fn from_clause(
        database: &mut Database,
        from: &BoundFromClause,
    ) -> Result<Self, DatabaseError> {
        match from {
            BoundFromClause::InnerJoin { left, on, right } => {
                let left = Self::from_clause(database, left)?;
                let right = Self::from_clause(database, right)?;

                let join_state = JoinScanState::new(left, right, on.clone());
                Ok(Self::HashJoin(join_state))
            }
            BoundFromClause::Table(table) => {
                let page_count = database
                    .table_manager
                    .page_count(&mut database.storage_manager, table.table_id)?;
                let scan_state = ScanState::new(table.clone(), page_count);
                Ok(Self::SeqScan(scan_state))
            }
        }
    }

    pub fn supports_from_clause(from: &BoundFromClause) -> bool {
        match from {
            BoundFromClause::InnerJoin { left, on, right } => {
                // 정방향 ex) users u join orders o on u.id = o.user_id
                let normal = Self::contains_instance(left, on.left.instance_id)
                    && Self::contains_instance(right, on.right.instance_id);

                // 역방향 ex) users u join orders o on o.user_id = u.id
                let reverse = Self::contains_instance(left, on.right.instance_id)
                    && Self::contains_instance(right, on.left.instance_id);

                Self::supports_from_clause(left)
                    && Self::supports_from_clause(right)
                    && (normal || reverse)
            }
            BoundFromClause::Table(table) => Self::contains_instance(from, table.instance_id),
        }
    }

    fn contains_instance(from: &BoundFromClause, instance_id: TableInstanceId) -> bool {
        match from {
            BoundFromClause::InnerJoin { left, right, .. } => {
                Self::contains_instance(left, instance_id)
                    || Self::contains_instance(right, instance_id)
            }
            BoundFromClause::Table(table) => table.instance_id == instance_id,
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
    pub fn new(bound: BoundSelect, input: InputState) -> Result<Self, ExecutorError> {
        Ok(Self::Scan {
            input,
            state: Box::new(SelectState::new(bound)?),
        })
    }

    pub fn new_scan(bound: BoundSelect, page_count: u64) -> Result<Self, ExecutorError> {
        let BoundFromClause::Table(table) = &bound.from else {
            return Err(ExecutorError::Unsupported);
        };

        let input = InputState::SeqScan(ScanState::new(table.clone(), page_count));
        Self::new(bound, input)
    }

    pub fn new_index_scan(bound: BoundSelect, row_ids: Vec<RowId>) -> Result<Self, ExecutorError> {
        let BoundFromClause::Table(table) = &bound.from else {
            return Err(ExecutorError::Unsupported);
        };

        let input = InputState::IndexScan(IndexScanState::new(table.clone(), row_ids));
        Self::new(bound, input)
    }
}
