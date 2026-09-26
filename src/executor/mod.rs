use std::collections::{HashMap, HashSet};

use crate::{
    catalog::metadata::{ColumnId, DatabaseMetadata, TableId},
    query::{
        binder::{
            BoundColumnReference, BoundDelete, BoundInsert, BoundJoinCondition, BoundProjection,
            BoundSelect, BoundTable, BoundUpdate, TableInstanceId,
        },
        common::ComparisonOperator,
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

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub table_id: TableId,
    pub row_id: RowId,
    pub new_row: Row,
    pub old_values: Vec<Value>,
    pub new_values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
struct PendingUpdate {
    pub table_id: TableId,
    pub row_id: RowId,
    pub old_values: Vec<Value>,
    pub new_values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedDelete {
    pub table_id: TableId,
    pub row_id: RowId,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct QueryRow {
    pub table_rows: Vec<TableRow>,
}

impl QueryRow {
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
        mut rows: Vec<QueryRow>,
        bound: &BoundUpdate,
    ) -> Result<Vec<PreparedUpdate>, ExecutorError> {
        if let Some(filter) = bound.filter.as_ref() {
            rows = self.filter_rows(rows, filter)?;
        }

        let mut pendings: Vec<PendingUpdate> = Vec::new();
        let mut positions: HashMap<(TableId, RowId), usize> = HashMap::new();
        for row in rows {
            for assignment in &bound.assignments {
                let table_row = row.find_table_row(assignment.column.instance_id).ok_or(
                    ExecutorError::TableInstanceNotFound(assignment.column.instance_id),
                )?;

                let table_meta = self
                    .database
                    .table_by_id(assignment.column.table_id)
                    .ok_or(ExecutorError::TableNotFound(assignment.column.table_id))?;

                let column_index = table_meta
                    .column_index(assignment.column.column_id)
                    .ok_or(ExecutorError::ColumnNotFound(assignment.column.column_id))?;

                let key = (assignment.column.table_id, table_row.row_id);
                if let Some(&index) = positions.get(&key) {
                    let prepared = &mut pendings[index];
                    prepared.new_values[column_index] = assignment.value.clone();
                } else {
                    let old_values = table_row.values.clone();
                    let mut new_values = old_values.clone();
                    new_values[column_index] = assignment.value.clone();

                    let index = pendings.len();
                    pendings.push(PendingUpdate {
                        table_id: assignment.column.table_id,
                        row_id: table_row.row_id,
                        old_values,
                        new_values,
                    });
                    positions.insert(key, index);
                }
            }
        }

        let mut results = Vec::new();
        for pending in pendings {
            let table_meta = self
                .database
                .table_by_id(pending.table_id)
                .ok_or(ExecutorError::TableNotFound(pending.table_id))?;

            let new_row = encode(&pending.new_values, table_meta.columns())?;
            results.push(PreparedUpdate {
                table_id: pending.table_id,
                row_id: pending.row_id,
                old_values: pending.old_values,
                new_values: pending.new_values,
                new_row,
            });
        }

        Ok(results)
    }

    pub fn prepare_delete(
        &self,
        mut rows: Vec<QueryRow>,
        bound: &BoundDelete,
    ) -> Result<Vec<PreparedDelete>, ExecutorError> {
        if let Some(filter) = bound.filter.as_ref() {
            rows = self.filter_rows(rows, filter)?;
        }

        let mut results: Vec<PreparedDelete> = Vec::new();
        let mut seen = HashSet::new();
        for row in rows {
            for target in &bound.targets {
                let table_row = row
                    .find_table_row(target.instance_id)
                    .ok_or(ExecutorError::TableInstanceNotFound(target.instance_id))?;

                if seen.insert((target.table_id, table_row.row_id)) {
                    results.push(PreparedDelete {
                        table_id: target.table_id,
                        row_id: table_row.row_id,
                        values: table_row.values.clone(),
                    });
                }
            }
        }

        Ok(results)
    }

    pub fn select_rows(
        &self,
        mut rows: Vec<QueryRow>,
        bound: &BoundSelect,
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        if let Some(filter) = bound.filter.as_ref() {
            rows = self.filter_rows(rows, filter)?;
        }

        if let Some(group_by) = bound.group_by.as_ref() {
            let mut groups = self.group_rows(rows, group_by)?;

            if let Some(order) = bound.order_by.as_ref() {
                groups = self.order_groups(groups, group_by, order)?;
            }

            let mut results = self.aggregate_groups(groups, group_by, &bound.projections)?;
            if let Some(limit) = bound.limit {
                results.truncate(limit);
            }
            return Ok(results);
        }

        if bound
            .projections
            .iter()
            .any(|projection| matches!(projection, BoundProjection::Aggregate(_)))
        {
            if bound.order_by.is_some() {
                return Err(ExecutorError::Unsupported);
            }

            let mut results = self.aggregate(rows, &bound.projections)?;
            if let Some(limit) = bound.limit {
                results.truncate(limit);
            }
            return Ok(results);
        }

        if let Some(order) = bound.order_by.as_ref() {
            rows = self.order_rows(rows, order)?;
        }

        let mut results = self.project_rows(&rows, &bound.projections)?;

        if let Some(limit) = bound.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    pub fn decode_table_rows(
        &self,
        table: &BoundTable,
        rows: Vec<(RowId, Row)>,
    ) -> Result<Vec<TableRow>, ExecutorError> {
        let table_meta = self
            .database
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

    pub fn nested_loop_join(
        &self,
        left_rows: &[QueryRow],
        right_rows: &[QueryRow],
        condition: &BoundJoinCondition,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut joined_rows = Vec::new();

        for left_row in left_rows {
            for right_row in right_rows {
                if self.matches_join_condition(left_row, right_row, condition)? {
                    let mut table_rows = left_row.table_rows.clone();
                    table_rows.extend(right_row.table_rows.iter().cloned());
                    joined_rows.push(QueryRow { table_rows });
                }
            }
        }

        Ok(joined_rows)
    }

    pub fn hash_join(
        &self,
        left_rows: &[QueryRow],
        right_rows: &[QueryRow],
        condition: &BoundJoinCondition,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut hash: HashMap<Value, Vec<usize>> = HashMap::new();
        let mut joined_rows = Vec::new();

        for (index, right_row) in right_rows.iter().enumerate() {
            let Some(column) = self.hash_join_column(right_row, condition) else {
                return self.nested_loop_join(left_rows, right_rows, condition);
            };

            let value = self.column_value(right_row, column)?;
            if *value != Value::Null {
                hash.entry(value.clone()).or_default().push(index);
            }
        }

        for left_row in left_rows {
            let Some(column) = self.hash_join_column(left_row, condition) else {
                return self.nested_loop_join(left_rows, right_rows, condition);
            };

            if let Some(right_indexes) = hash.get(self.column_value(left_row, column)?) {
                for right_index in right_indexes {
                    let mut table_rows = left_row.table_rows.clone();
                    table_rows.extend(right_rows[*right_index].table_rows.iter().cloned());
                    joined_rows.push(QueryRow { table_rows });
                }
            }
        }

        Ok(joined_rows)
    }

    fn matches_join_condition(
        &self,
        left_row: &QueryRow,
        right_row: &QueryRow,
        condition: &BoundJoinCondition,
    ) -> Result<bool, ExecutorError> {
        let left_value = self.join_column_value(left_row, right_row, &condition.left)?;
        let right_value = self.join_column_value(left_row, right_row, &condition.right)?;

        Ok(left_value.compare(ComparisonOperator::Equal, right_value))
    }

    fn join_column_value<'row>(
        &self,
        left_row: &'row QueryRow,
        right_row: &'row QueryRow,
        column: &BoundColumnReference,
    ) -> Result<&'row Value, ExecutorError> {
        let source = if left_row.find_table_row(column.instance_id).is_some() {
            left_row
        } else if right_row.find_table_row(column.instance_id).is_some() {
            right_row
        } else {
            return Err(ExecutorError::TableInstanceNotFound(column.instance_id));
        };

        self.column_value(source, column)
    }

    fn hash_join_column<'row>(
        &self,
        row: &'row QueryRow,
        condition: &'row BoundJoinCondition,
    ) -> Option<&'row BoundColumnReference> {
        if row.find_table_row(condition.left.instance_id).is_some() {
            Some(&condition.left)
        } else if row.find_table_row(condition.right.instance_id).is_some() {
            Some(&condition.right)
        } else {
            None
        }
    }

    fn column_value<'row>(
        &self,
        row: &'row QueryRow,
        column: &BoundColumnReference,
    ) -> Result<&'row Value, ExecutorError> {
        let table_row = row
            .find_table_row(column.instance_id)
            .ok_or(ExecutorError::TableInstanceNotFound(column.instance_id))?;

        let table_meta = self
            .database
            .table_by_id(column.table_id)
            .ok_or(ExecutorError::TableNotFound(column.table_id))?;

        let column_index = table_meta
            .column_index(column.column_id)
            .ok_or(ExecutorError::ColumnNotFound(column.column_id))?;
        table_row
            .values
            .get(column_index)
            .ok_or(ExecutorError::ColumnNotFound(column.column_id))
    }
}

#[cfg(test)]
mod tests;
