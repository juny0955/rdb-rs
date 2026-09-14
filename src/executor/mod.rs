use crate::{
    binder::{BoundDelete, BoundInsert, BoundSelect, BoundUpdate},
    catalog::metadata::{ColumnId, DatabaseMetadata, TableId},
    executor::{
        predicate::{filter_rows, order_rows},
        projection::project_rows,
    },
    storage::page::{Row, RowId},
    tuple::{TupleError, Value, decode, encode},
};
use thiserror::Error;

mod predicate;
mod projection;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error(transparent)]
    Tuple(#[from] TupleError),
    #[error("테이블을 찾을 수 없습니다: {0:?}")]
    TableNotFound(TableId),
    #[error("컬럼을 찾을 수 없습니다: {0:?}")]
    ColumnNotFound(ColumnId),
}

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub row_id: RowId,
    pub new_row: Row,
    pub old_values: Vec<Value>,
    pub new_values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedDelete {
    pub row_id: RowId,
    pub values: Vec<Value>,
}

pub struct Executor<'a> {
    database: &'a DatabaseMetadata,
}

impl<'a> Executor<'a> {
    pub fn new(database: &'a DatabaseMetadata) -> Self {
        Self { database }
    }

    pub fn encode_insert(&self, bound: &BoundInsert) -> Result<Row, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        Ok(encode(&bound.values, table.columns())?)
    }

    pub fn prepare_update(
        &self,
        bound: &BoundUpdate,
        rows: Vec<(RowId, Row)>,
    ) -> Result<Vec<PreparedUpdate>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let mut values = decode(&row, table.columns())?;
            let old_values = values.clone();
            for assignment in &bound.assignments {
                let column_index = table
                    .column_index(assignment.column_id)
                    .ok_or(ExecutorError::ColumnNotFound(assignment.column_id))?;

                values[column_index] = assignment.value.clone();
            }
            let new_values = values.clone();
            let new_row = encode(&new_values, table.columns())?;
            results.push(PreparedUpdate {
                row_id,
                new_row,
                old_values,
                new_values,
            });
        }

        Ok(results)
    }

    pub fn prepare_delete(
        &self,
        bound: &BoundDelete,
        rows: Vec<(RowId, Row)>,
    ) -> Result<Vec<PreparedDelete>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let values = decode(&row, table.columns())?;
            results.push(PreparedDelete { row_id, values });
        }

        Ok(results)
    }

    pub fn select_rows(
        &self,
        mut rows: Vec<(RowId, Row)>,
        bound: &BoundSelect,
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let table = self
            .database
            .table_by_id(bound.table_id)
            .ok_or(ExecutorError::TableNotFound(bound.table_id))?;

        if let Some(filter) = bound.filter.as_ref() {
            rows = filter_rows(rows, table, filter)?;
        }

        if let Some(order) = bound.order_by.as_ref() {
            rows = order_rows(rows, table, order)?;
        }

        if let Some(limit) = bound.limit {
            rows.truncate(limit);
        }

        project_rows(rows, table, &bound.projections)
    }
}

#[cfg(test)]
mod tests;
