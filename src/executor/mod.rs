use crate::{
    binder::{
        BoundDelete, BoundExpression, BoundInsert, BoundProjection, BoundSelect, BoundUpdate,
    },
    catalog::metadata::{ColumnId, DatabaseMetadata, TableId, TableMetadata},
    storage::buffer::BufferPool,
    storage::heap::{HeapTable, HeapTableError},
    storage::page::{Row, RowId},
    tuple::{TupleError, Value, decode, encode},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("tuple 처리 오류: {0}")]
    Tuple(#[from] TupleError),
    #[error("heap table 처리 오류: {0}")]
    HeapTable(#[from] HeapTableError),
    #[error("테이블을 찾을 수 없습니다: {0:?}")]
    TableNotFound(TableId),
    #[error("컬럼을 찾을 수 없습니다: {0:?}")]
    ColumnNotFound(ColumnId),
}

#[derive(Debug, PartialEq, Eq)]
pub struct InsertResult {
    pub row_id: RowId,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateResult {
    pub row_id: RowId,
    pub old_values: Vec<Value>,
    pub new_values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeleteResult {
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

    pub fn execute_select(
        &self,
        bound: &BoundSelect,
        heap_table: &mut HeapTable,
        buffer_pool: &mut BufferPool,
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let rows = heap_table.scan(buffer_pool)?;
        self.projection_and_filtered_rows(rows, bound)
    }

    pub fn projection_and_filtered_rows(
        &self,
        rows: Vec<(RowId, Row)>,
        bound: &BoundSelect,
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let table = self
            .database
            .table_by_id(bound.table_id)
            .ok_or(ExecutorError::TableNotFound(bound.table_id))?;

        let projections = &bound.projections;
        let Some(filter) = bound.filter.as_ref() else {
            return Self::project_rows(rows, table, projections);
        };

        let filtered_rows = Self::filter_rows(rows, table, filter)?;
        Self::project_rows(filtered_rows, table, projections)
    }

    pub fn execute_insert(
        &self,
        bound: &BoundInsert,
        heap_table: &mut HeapTable,
        buffer_pool: &mut BufferPool,
    ) -> Result<InsertResult, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let row = encode(&bound.values, table.columns())?;
        let row_id = heap_table.insert(&row, buffer_pool)?;

        Ok(InsertResult {
            row_id,
            values: bound.values.to_owned(),
        })
    }

    pub fn execute_update(
        &self,
        bound: &BoundUpdate,
        heap_table: &mut HeapTable,
        buffer_pool: &mut BufferPool,
    ) -> Result<Vec<UpdateResult>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let rows = heap_table.scan(buffer_pool)?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                Self::filter_rows(rows, table, filter)?
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
            let row = encode(&values, table.columns())?;
            heap_table.update(row_id, &row, buffer_pool)?;
            results.push(UpdateResult {
                row_id,
                old_values,
                new_values,
            });
        }

        Ok(results)
    }

    pub fn execute_delete(
        &self,
        bound: &BoundDelete,
        heap_table: &mut HeapTable,
        buffer_pool: &mut BufferPool,
    ) -> Result<Vec<DeleteResult>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let rows = heap_table.scan(buffer_pool)?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                Self::filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let values = decode(&row, table.columns())?;
            heap_table.delete(row_id, buffer_pool)?;
            results.push(DeleteResult { row_id, values });
        }

        Ok(results)
    }

    fn filter_rows(
        rows: Vec<(RowId, Row)>,
        table: &TableMetadata,
        filter: &BoundExpression,
    ) -> Result<Vec<(RowId, Row)>, ExecutorError> {
        let BoundExpression::Equal { column_id, value } = filter;
        let column_index = table
            .column_index(*column_id)
            .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let values = decode(&row, table.columns())?;

            if sql_equals(&values[column_index], value) {
                results.push((row_id, row));
            }
        }

        Ok(results)
    }

    fn project_rows(
        rows: Vec<(RowId, Row)>,
        table: &TableMetadata,
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();

        for (_, row) in rows {
            let values = decode(&row, table.columns())?;
            let mut projection_values = Vec::new();

            for projection in projections {
                match projection {
                    BoundProjection::All => projection_values.extend(values.iter().cloned()),
                    BoundProjection::Column(column_id) => {
                        let column_index = table
                            .column_index(*column_id)
                            .ok_or(ExecutorError::ColumnNotFound(*column_id))?;
                        projection_values.push(values[column_index].clone());
                    }
                }
            }

            results.push(projection_values);
        }

        Ok(results)
    }
}

fn sql_equals(left: &Value, right: &Value) -> bool {
    if left == &Value::Null || right == &Value::Null {
        return false;
    }

    left == right
}

#[cfg(test)]
mod tests;
