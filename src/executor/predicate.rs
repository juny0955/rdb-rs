use std::cmp::Ordering;

use crate::{
    binder::{BoundExpression, BoundOrderBy, BoundSortedDirection},
    catalog::metadata::TableMetadata,
    executor::ExecutorError,
    storage::page::{Row, RowId},
    tuple::{Value, decode},
};

pub(super) fn filter_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    filter: &BoundExpression,
) -> Result<Vec<(RowId, Row)>, ExecutorError> {
    let mut results = Vec::new();
    for (row_id, row) in rows {
        let values = decode(&row, table.columns())?;
        if row_matches_filter(&values, table, filter)? {
            results.push((row_id, row));
        }
    }

    Ok(results)
}

pub(super) fn order_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    order: &BoundOrderBy,
) -> Result<Vec<(RowId, Row)>, ExecutorError> {
    let column_index = table
        .column_index(order.column_id)
        .ok_or(ExecutorError::ColumnNotFound(order.column_id))?;

    let mut keyed_rows = Vec::new();
    for (row_id, row) in rows {
        let values = decode(&row, table.columns())?;
        let key = values[column_index].clone();
        keyed_rows.push((key, row_id, row));
    }

    keyed_rows.sort_by(|left, right| match (&left.0, &right.0) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::BigInt(a), Value::BigInt(b)) => a.cmp(b),
        (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
        _ => Ordering::Equal,
    });

    if order.direction == BoundSortedDirection::Desc {
        keyed_rows.reverse();
    }

    let results = keyed_rows.into_iter().map(|r| (r.1, r.2)).collect();

    Ok(results)
}

fn row_matches_filter(
    values: &[Value],
    table: &TableMetadata,
    filter: &BoundExpression,
) -> Result<bool, ExecutorError> {
    match filter {
        BoundExpression::Comparison {
            column_id,
            operator,
            value,
        } => {
            let column_index = table
                .column_index(*column_id)
                .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

            Ok(values[column_index].compare(operator, value))
        }
        BoundExpression::And { left, right } => {
            Ok(row_matches_filter(values, table, left)?
                && row_matches_filter(values, table, right)?)
        }
        BoundExpression::Or { left, right } => {
            Ok(row_matches_filter(values, table, left)?
                || row_matches_filter(values, table, right)?)
        }
    }
}
