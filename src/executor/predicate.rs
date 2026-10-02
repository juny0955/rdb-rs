use std::cmp::Ordering;

use crate::{
    executor::{Executor, ExecutorError, QueryRow, projection::RowGroup},
    query::{
        binder::{BoundColumnReference, BoundExpression, BoundOrderBy},
        common::SortDirection,
    },
    tuple::Value,
};

impl<'a> Executor<'a> {
    pub(super) fn filter_rows(
        &self,
        rows: Vec<QueryRow>,
        filter: &BoundExpression,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut results = Vec::new();

        for row in rows {
            if self.row_matches_filter(&row, filter)? {
                results.push(row);
            }
        }

        Ok(results)
    }

    pub(crate) fn row_matches_filter(
        &self,
        row: &QueryRow,
        filter: &BoundExpression,
    ) -> Result<bool, ExecutorError> {
        Ok(match filter {
            BoundExpression::And { left, right } => {
                self.row_matches_filter(row, left)? && self.row_matches_filter(row, right)?
            }
            BoundExpression::Or { left, right } => {
                self.row_matches_filter(row, left)? || self.row_matches_filter(row, right)?
            }
            BoundExpression::Comparison {
                column,
                operator,
                value,
            } => {
                let column_value = self.column_value(row, column)?;
                column_value.compare(*operator, value)
            }
        })
    }

    pub(super) fn order_rows(
        &self,
        rows: Vec<QueryRow>,
        order: &BoundOrderBy,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut keyed_rows = Vec::new();
        for row in rows {
            let key = self.column_value(&row, &order.column)?;
            keyed_rows.push((key.clone(), row));
        }

        keyed_rows.sort_by(|left, right| compare_values(&left.0, &right.0));

        if order.direction == SortDirection::Desc {
            keyed_rows.reverse();
        }

        let results = keyed_rows.into_iter().map(|r| r.1).collect();

        Ok(results)
    }

    pub(super) fn order_groups(
        &self,
        mut groups: Vec<RowGroup>,
        group_by: &[BoundColumnReference],
        order: &BoundOrderBy,
    ) -> Result<Vec<RowGroup>, ExecutorError> {
        let key_index = group_by
            .iter()
            .position(|group_column_id| group_column_id == &order.column)
            .ok_or(ExecutorError::Unsupported)?;

        groups.sort_by(|left, right| compare_values(&left.key[key_index], &right.key[key_index]));

        if order.direction == SortDirection::Desc {
            groups.reverse();
        }

        Ok(groups)
    }
}

fn compare_values(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::BigInt(a), Value::BigInt(b)) => a.cmp(b),
        (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}
