use std::collections::HashMap;

use crate::{
    catalog::metadata::{DatabaseMetadata, TableId},
    executor::{ExecutorError, QueryRow, predicate::row_matches_filter},
    query::binder::BoundUpdate,
    storage::page::{Row, RowId},
    tuple::{Value, encode},
};

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

pub struct UpdateState {
    bound: BoundUpdate,
    pendings: Vec<PendingUpdate>,
    positions: HashMap<(TableId, RowId), usize>,
}

impl UpdateState {
    pub fn new(bound: BoundUpdate) -> Self {
        Self {
            bound,
            pendings: Vec::new(),
            positions: HashMap::new(),
        }
    }

    pub fn prepare_row(
        &mut self,
        metadata: &DatabaseMetadata,
        row: &QueryRow,
    ) -> Result<(), ExecutorError> {
        if let Some(filter) = self.bound.filter.as_ref()
            && !row_matches_filter(metadata, row, filter)?
        {
            return Ok(());
        }

        for assignment in &self.bound.assignments {
            let table_row = row.find_table_row(assignment.column.instance_id).ok_or(
                ExecutorError::TableInstanceNotFound(assignment.column.instance_id),
            )?;

            let table_meta = metadata
                .table_by_id(assignment.column.table_id)
                .ok_or(ExecutorError::TableNotFound(assignment.column.table_id))?;

            let column_index = table_meta
                .column_index(assignment.column.column_id)
                .ok_or(ExecutorError::ColumnNotFound(assignment.column.column_id))?;

            let key = (assignment.column.table_id, table_row.row_id);
            if let Some(&index) = self.positions.get(&key) {
                let prepared = &mut self.pendings[index];
                prepared.new_values[column_index] = assignment.value.clone();
            } else {
                let old_values = table_row.values.clone();
                let mut new_values = old_values.clone();
                new_values[column_index] = assignment.value.clone();

                let index = self.pendings.len();
                self.pendings.push(PendingUpdate {
                    table_id: assignment.column.table_id,
                    row_id: table_row.row_id,
                    old_values,
                    new_values,
                });
                self.positions.insert(key, index);
            }
        }

        Ok(())
    }

    pub fn finish(
        &mut self,
        metadata: &DatabaseMetadata,
    ) -> Result<Vec<PreparedUpdate>, ExecutorError> {
        let mut results = Vec::new();
        let pendings = std::mem::take(&mut self.pendings);
        self.positions.clear();

        for pending in pendings {
            let table_meta = metadata
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
}
