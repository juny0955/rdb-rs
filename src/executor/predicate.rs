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
    let BoundExpression::Equal { column_id, value } = filter;
    let column_index = table
        .column_index(*column_id)
        .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

    let mut results = Vec::new();
    for (row_id, row) in rows {
        let values = decode(&row, table.columns())?;

        if sql_equals(&values[column_index], value) {
            results.push((row_id, row));
        }
    }

    Ok(results)
}

pub(super) fn sql_equals(left: &Value, right: &Value) -> bool {
    if left == &Value::Null || right == &Value::Null {
        return false;
    }

    left == right
}
