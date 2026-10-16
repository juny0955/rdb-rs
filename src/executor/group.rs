use std::collections::HashMap;

use crate::{
    executor::{
        Executor, ExecutorError, QueryRow, aggregate::AggregateState, order::compare_values,
    },
    query::binder::{BoundColumnReference, BoundOrderBy, BoundProjection},
    tuple::Value,
};

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
        mut self,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
        order_by: Option<&BoundOrderBy>,
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();

        if let Some(order) = order_by {
            let key_index = group_by
                .iter()
                .position(|column| column == &order.column)
                .ok_or(ExecutorError::Unsupported)?;

            self.groups
                .sort_by(|left, right| compare_values(&left.key[key_index], &right.key[key_index]));

            if order.direction.is_desc() {
                self.groups.reverse();
            }
        }

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

#[cfg(test)]
#[path = "tests/group_state.rs"]
mod tests;
