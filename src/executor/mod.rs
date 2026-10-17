use crate::{
    catalog::metadata::{ColumnId, DatabaseMetadata, TableId},
    query::binder::{BoundColumnReference, BoundInsert, BoundTable, TableInstanceId},
    storage::page::{Row, RowId},
    tuple::{TupleError, Value, decode, encode},
};
use thiserror::Error;

pub mod aggregate;
mod group;
pub mod join;
mod order;
mod predicate;
mod projection;
pub mod state;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error(transparent)]
    Tuple(#[from] TupleError),
    #[error("테이블을 찾을 수 없습니다: {0:?}")]
    TableNotFound(TableId),
    #[error("테이블 인스턴스를 찾을 수 없습니다: {0:?}")]
    TableInstanceNotFound(TableInstanceId),
    #[error("컬럼을 찾을 수 없습니다: {0:?}")]
    ColumnNotFound(ColumnId),
    #[error("COUNT 결과가 BIGINT 범위를 초과했습니다: {count}")]
    CountOutOfRange { count: usize },
    #[error("SUM 결과가 BIGINT 범위를 초과했습니다")]
    SumOverflow,
    #[error("현재 쿼리 형태는 실행기에서 지원하지 않습니다")]
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct QueryRow {
    pub table_rows: Vec<TableRow>,
}

impl QueryRow {
    pub fn new(table_rows: Vec<TableRow>) -> Self {
        Self { table_rows }
    }

    pub fn find_table_row(&self, instance_id: TableInstanceId) -> Option<&TableRow> {
        self.table_rows
            .iter()
            .find(|table_row| table_row.instance_id == instance_id)
    }
}

#[derive(Debug, Clone)]
pub struct TableRow {
    pub row_id: RowId,
    pub instance_id: TableInstanceId,
    pub values: Vec<Value>,
}

pub fn encode_insert(
    metadata: &DatabaseMetadata,
    bound: &BoundInsert,
) -> Result<Row, ExecutorError> {
    let table_meta = metadata
        .table_by_id(bound.table_id)
        .ok_or(ExecutorError::TableNotFound(bound.table_id))?;

    Ok(encode(&bound.values, table_meta.columns())?)
}

pub fn decode_table_rows(
    metadata: &DatabaseMetadata,
    table: &BoundTable,
    rows: Vec<(RowId, Row)>,
) -> Result<Vec<TableRow>, ExecutorError> {
    let table_meta = metadata
        .table_by_id(table.table_id)
        .ok_or(ExecutorError::TableNotFound(table.table_id))?;

    let mut table_rows = Vec::new();
    for (row_id, row) in rows {
        table_rows.push(TableRow {
            row_id,
            instance_id: table.instance_id,
            values: decode(&row, table_meta.columns())?,
        });
    }

    Ok(table_rows)
}

pub fn column_value<'row>(
    metadata: &DatabaseMetadata,
    row: &'row QueryRow,
    column: &BoundColumnReference,
) -> Result<&'row Value, ExecutorError> {
    let table_row = row
        .find_table_row(column.instance_id)
        .ok_or(ExecutorError::TableInstanceNotFound(column.instance_id))?;

    let column_index = metadata
        .table_by_id(column.table_id)
        .ok_or(ExecutorError::TableNotFound(column.table_id))?
        .column_index(column.column_id)
        .ok_or(ExecutorError::ColumnNotFound(column.column_id))?;

    table_row
        .values
        .get(column_index)
        .ok_or(ExecutorError::ColumnNotFound(column.column_id))
}

#[cfg(test)]
mod tests;
