use std::collections::HashMap;

use crate::{
    executor::{Executor, ExecutorError, QueryRow, aggregate::AggregateState},
    query::binder::{BoundColumnReference, BoundProjection},
    tuple::Value,
};

pub(super) struct RowGroup {
    pub key: Vec<Value>,
    pub rows: Vec<QueryRow>,
}

impl<'a> Executor<'a> {
    pub(crate) fn project_row(
        &self,
        row: &QueryRow,
        projections: &[BoundProjection],
    ) -> Result<Vec<Value>, ExecutorError> {
        let mut results = Vec::new();

        for projection in projections {
            match projection {
                BoundProjection::All => {
                    for table_row in &row.table_rows {
                        results.extend(table_row.values.clone());
                    }
                }
                BoundProjection::Column(column) => {
                    let value = self.column_value(row, column)?;
                    results.push(value.clone());
                }
                BoundProjection::Aggregate(_) => return Err(ExecutorError::Unsupported),
            }
        }

        Ok(results)
    }

    pub(super) fn project_rows(
        &self,
        rows: &[QueryRow],
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();
        for row in rows {
            let values = self.project_row(row, projections)?;
            results.push(values);
        }

        Ok(results)
    }

    pub(super) fn group_rows(
        &self,
        rows: Vec<QueryRow>,
        group_by: &[BoundColumnReference],
    ) -> Result<Vec<RowGroup>, ExecutorError> {
        let mut groups: Vec<RowGroup> = Vec::new();
        let mut indices: HashMap<Vec<Value>, usize> = HashMap::new();
        for row in rows {
            let mut key = Vec::new();
            for column_ref in group_by {
                let value = self.column_value(&row, column_ref)?;
                key.push(value.clone());
            }

            if let Some(&group_index) = indices.get(&key) {
                groups[group_index].rows.push(row);
            } else {
                indices.insert(key.clone(), groups.len());
                groups.push(RowGroup {
                    key,
                    rows: vec![row],
                });
            }
        }

        Ok(groups)
    }

    pub(crate) fn aggregate(
        &self,
        rows: Vec<QueryRow>,
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let group = RowGroup { key: vec![], rows };
        self.aggregate_groups(vec![group], &[], projections)
    }

    pub(super) fn aggregate_groups(
        &self,
        groups: Vec<RowGroup>,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();
        for group in groups {
            results.push(self.aggregate_group(&group, group_by, projections)?);
        }

        Ok(results)
    }

    fn aggregate_group(
        &self,
        group: &RowGroup,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
    ) -> Result<Vec<Value>, ExecutorError> {
        let mut projection_result = Vec::new();
        for projection in projections {
            match projection {
                BoundProjection::Column(id) => {
                    let key_index = group_by
                        .iter()
                        .position(|group_column_id| group_column_id == id)
                        .ok_or(ExecutorError::Unsupported)?;

                    projection_result.push(group.key[key_index].clone());
                }
                BoundProjection::Aggregate(aggregate) => {
                    let mut state = AggregateState::new(aggregate);
                    for row in &group.rows {
                        self.accumulate_aggregate(&mut state, projection, row)?;
                    }
                    projection_result.push(state.result()?);
                }
                _ => return Err(ExecutorError::Unsupported),
            }
        }

        Ok(projection_result)
    }
}
