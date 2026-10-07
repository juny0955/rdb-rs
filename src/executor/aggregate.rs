use crate::{
    executor::{Executor, ExecutorError, QueryRow, group::RowGroup},
    query::binder::{BoundAggregate, BoundColumnReference, BoundProjection},
    tuple::Value,
};

pub enum AggregateState {
    CountAll { count: usize },
    Sum { total: Option<i64> },
}

impl AggregateState {
    pub fn new(aggregate: &BoundAggregate) -> Self {
        match aggregate {
            BoundAggregate::CountAll => Self::CountAll { count: 0 },
            BoundAggregate::Sum(_) => Self::Sum { total: None },
        }
    }

    pub fn result(&self) -> Result<Value, ExecutorError> {
        Ok(match self {
            Self::CountAll { count } => Value::BigInt(
                i64::try_from(*count)
                    .map_err(|_| ExecutorError::CountOutOfRange { count: *count })?,
            ),
            Self::Sum { total } => total.map(Value::BigInt).unwrap_or(Value::Null),
        })
    }
}

impl Executor<'_> {
    pub(crate) fn accumulate_aggregate(
        &self,
        state: &mut AggregateState,
        projection: &BoundProjection,
        row: &QueryRow,
    ) -> Result<(), ExecutorError> {
        match state {
            AggregateState::CountAll { count } => *count += 1,
            AggregateState::Sum { total } => {
                let BoundProjection::Aggregate(BoundAggregate::Sum(column)) = projection else {
                    return Err(ExecutorError::Unsupported);
                };

                let addend = match self.column_value(row, column)? {
                    Value::Int(value) => i64::from(*value),
                    Value::BigInt(value) => *value,
                    Value::Null => return Ok(()),
                    _ => return Err(ExecutorError::Unsupported),
                };

                *total = Some(
                    (*total)
                        .unwrap_or(0)
                        .checked_add(addend)
                        .ok_or(ExecutorError::SumOverflow)?,
                );
            }
        }

        Ok(())
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
