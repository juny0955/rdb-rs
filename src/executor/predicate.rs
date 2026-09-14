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
