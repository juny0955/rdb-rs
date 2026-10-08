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

pub(super) struct GroupAccumulator {
    pub key: Vec<Value>,
    pub aggregates: Vec<AggregateState>,
}

pub(super) struct GroupState {
    groups: Vec<GroupAccumulator>,
    indices: HashMap<Vec<Value>, usize>,
}

impl GroupState {
    pub(super) fn new() -> Self {
        Self {
            groups: Vec::new(),
            indices: HashMap::new(),
        }
    }

    pub(super) fn push_row(
        &mut self,
        executor: &Executor<'_>,
        row: &QueryRow,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
    ) -> Result<(), ExecutorError> {
        let mut key = Vec::new();
        for column_ref in group_by {
            let value = executor.column_value(row, column_ref)?;
            key.push(value.clone());
        }

        let group_index = if let Some(group_index) = self.indices.get(&key) {
            *group_index
        } else {
            let mut aggregates = Vec::new();
            for projection in projections {
                match projection {
                    BoundProjection::Aggregate(aggreate) => {
                        aggregates.push(AggregateState::new(aggreate));
                    }
                    BoundProjection::Column(_) => continue,
                    BoundProjection::All => return Err(ExecutorError::Unsupported),
                }
            }

            let group_index = self.groups.len();
            self.indices.insert(key.clone(), group_index);
            self.groups.push(GroupAccumulator {
                key: key.clone(),
                aggregates,
            });

            group_index
        };

        let aggregate_projections = projections
            .iter()
            .filter(|projection| matches!(projection, BoundProjection::Aggregate(_)));
        let accumulator = &mut self.groups[group_index];

        for (projection, state) in aggregate_projections.zip(accumulator.aggregates.iter_mut()) {
            executor.accumulate_aggregate(state, projection, row)?;
        }

        Ok(())
    }

    pub(super) fn into_rows(
        self,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();

        for group in self.groups {
            let mut values = Vec::new();
            let mut aggreate_index = 0;

            for projection in projections {
                match projection {
                    BoundProjection::Aggregate(_) => {
                        let value = group.aggregates[aggreate_index].result()?;
                        values.push(value);
                        aggreate_index += 1;
                    }
                    BoundProjection::Column(column) => {
                        let key_index = group_by
                            .iter()
                            .position(|group_column| group_column == column)
                            .ok_or(ExecutorError::Unsupported)?;

                        values.push(group.key[key_index].clone());
                    }
                    BoundProjection::All => return Err(ExecutorError::Unsupported),
                }
            }
            results.push(values);
        }

        Ok(results)
    }
}

impl<'a> Executor<'a> {
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
}

#[cfg(test)]
#[path = "tests/group_state.rs"]
mod tests;
