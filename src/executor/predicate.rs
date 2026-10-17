use crate::{
    catalog::metadata::DatabaseMetadata,
    executor::{ExecutorError, QueryRow, column_value},
    query::binder::BoundExpression,
};

pub(crate) fn row_matches_filter(
    metadata: &DatabaseMetadata,
    row: &QueryRow,
    filter: &BoundExpression,
) -> Result<bool, ExecutorError> {
    Ok(match filter {
        BoundExpression::And { left, right } => {
            row_matches_filter(metadata, row, left)? && row_matches_filter(metadata, row, right)?
        }
        BoundExpression::Or { left, right } => {
            row_matches_filter(metadata, row, left)? || row_matches_filter(metadata, row, right)?
        }
        BoundExpression::Comparison {
            column,
            operator,
            value,
        } => {
            let column_value = column_value(metadata, row, column)?;
            column_value.compare(*operator, value)
        }
    })
}
