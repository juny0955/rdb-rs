use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::{BoundAggregate, BoundProjection},
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
}
