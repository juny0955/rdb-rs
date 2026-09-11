use crate::{
    binder::BoundExpression,
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

fn row_matches_filter(
    values: &[Value],
    table: &TableMetadata,
    filter: &BoundExpression,
) -> Result<bool, ExecutorError> {
    match filter {
        BoundExpression::Equal { column_id, value } => {
            let column_index = table
                .column_index(*column_id)
                .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

            Ok(sql_equals(&values[column_index], value))
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

pub(super) fn sql_equals(left: &Value, right: &Value) -> bool {
    if left == &Value::Null || right == &Value::Null {
        return false;
    }

    left == right
}
