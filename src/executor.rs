use std::path::{Path, PathBuf};

use crate::{
    binder::{
        BoundDelete, BoundExpression, BoundInsert, BoundProjection, BoundSelect, BoundUpdate,
    },
    page::{Row, RowId},
    parser::ast::Literal,
    schema::{ColumnId, DataType, DatabaseMetadata, TableId, TableMetadata},
    table::{HeapTable, HeapTableError},
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
    #[error("리터럴 타입이 올바르지 않습니다 (기대 타입: {expected:?})")]
    LiteralTypeMismatch { expected: DataType },
}

pub struct Executor<'a> {
    database: &'a DatabaseMetadata,
    table_dir: &'a Path,
}

impl<'a> Executor<'a> {
    pub fn new(database: &'a DatabaseMetadata, table_dir: &'a Path) -> Self {
        Self {
            database,
            table_dir,
        }
    }

    pub fn execute_select(&self, bound: &BoundSelect) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let rows = heap_table.scan()?;

        let projections = &bound.projections;
        let Some(filter) = bound.filter.as_ref() else {
            return Self::project_rows(rows, table, projections);
        };

        let filtered_rows = Self::filter_rows(rows, table, filter)?;
        Self::project_rows(filtered_rows, table, projections)
    }

    pub fn execute_insert(&self, bound: &BoundInsert) -> Result<RowId, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut values = Vec::new();
        for (literal, column) in bound.literals.iter().zip(table.columns()) {
            let value = Self::literal_to_value(literal, column.data_type())?;
            values.push(value);
        }
        let row = encode(&values, table.columns())?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let row_id = heap_table.insert(&row)?;

        Ok(row_id)
    }

    pub fn execute_update(&self, bound: &BoundUpdate) -> Result<usize, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let rows = heap_table.scan()?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                Self::filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut updated = 0;
        for (row_id, row) in rows {
            let mut values = decode(&row, table.columns())?;
            for assignment in &bound.assignments {
                let column_index = table
                    .column_index(assignment.column_id)
                    .ok_or(ExecutorError::ColumnNotFound(assignment.column_id))?;
                let column = &table.columns()[column_index];

                let value = Self::literal_to_value(&assignment.value, column.data_type())?;
                values[column_index] = value;
            }
            let row = encode(&values, table.columns())?;
            heap_table.update(row_id, &row)?;
            updated += 1;
        }

        Ok(updated)
    }

    pub fn execute_delete(&self, bound: &BoundDelete) -> Result<usize, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let rows = heap_table.scan()?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                Self::filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut deleted = 0;
        for (row_id, _) in rows {
            heap_table.delete(row_id)?;
            deleted += 1;
        }

        Ok(deleted)
    }

    fn filter_rows(
        rows: Vec<(RowId, Row)>,
        table: &TableMetadata,
        filter: &BoundExpression,
    ) -> Result<Vec<(RowId, Row)>, ExecutorError> {
        let BoundExpression::Equal { column_id, value } = filter;
        let literal = value;
        let column_index = table
            .column_index(*column_id)
            .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let values = decode(&row, table.columns())?;

            if Self::is_equal(&values[column_index], literal) {
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

    fn is_equal(value: &Value, literal: &Literal) -> bool {
        match (value, literal) {
            (Value::Null, Literal::Null) => false,
            (Value::Int(a), Literal::Integer(b)) => i64::from(*a) == *b,
            (Value::BigInt(a), Literal::Integer(b)) => a == b,
            (Value::Varchar(a), Literal::String(b)) => a == b,
            _ => false,
        }
    }

    fn literal_to_value(literal: &Literal, data_type: DataType) -> Result<Value, ExecutorError> {
        Ok(match (literal, data_type) {
            (Literal::Null, _) => Value::Null,
            (Literal::Integer(a), DataType::Int) => Value::Int(i32::try_from(*a).map_err(
                |_| ExecutorError::LiteralTypeMismatch {
                    expected: data_type,
                },
            )?),
            (Literal::Integer(a), DataType::BigInt) => Value::BigInt(*a),
            (Literal::String(a), DataType::Varchar) => Value::Varchar(a.to_owned()),
            _ => {
                return Err(ExecutorError::LiteralTypeMismatch {
                    expected: data_type,
                });
            }
        })
    }

    fn table_path(&self, table_id: TableId) -> PathBuf {
        self.table_dir.join(format!("{}.tbl", table_id.id()))
    }
}

#[cfg(test)]
mod tests;
