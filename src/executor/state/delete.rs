use std::collections::HashSet;

use crate::{
    catalog::metadata::TableId,
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::BoundDelete,
    storage::page::RowId,
    tuple::Value,
};

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedDelete {
    pub table_id: TableId,
    pub row_id: RowId,
    pub values: Vec<Value>,
}

pub struct DeleteState {
    bound: BoundDelete,
    seen: HashSet<(TableId, RowId)>,
}

impl DeleteState {
    pub fn new(bound: BoundDelete) -> Self {
        Self {
            bound,
            seen: HashSet::new(),
        }
    }

    pub fn prepare_row(
        &mut self,
        executor: &Executor<'_>,
        row: &QueryRow,
    ) -> Result<Vec<PreparedDelete>, ExecutorError> {
        if let Some(filter) = self.bound.filter.as_ref()
            && !executor.row_matches_filter(row, filter)?
        {
            return Ok(vec![]);
        }

        let mut results: Vec<PreparedDelete> = Vec::new();
        for target in &self.bound.targets {
            let table_row = row
                .find_table_row(target.instance_id)
                .ok_or(ExecutorError::TableInstanceNotFound(target.instance_id))?;

            if self.seen.insert((target.table_id, table_row.row_id)) {
                results.push(PreparedDelete {
                    table_id: target.table_id,
                    row_id: table_row.row_id,
                    values: table_row.values.clone(),
                });
            }
        }

        Ok(results)
    }

    pub fn into_targets(self) -> HashSet<(TableId, RowId)> {
        self.seen
    }
}
