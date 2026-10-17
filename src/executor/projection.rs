use crate::{
    catalog::metadata::DatabaseMetadata,
    executor::{ExecutorError, QueryRow, column_value},
    query::binder::BoundProjection,
    tuple::Value,
};

pub(crate) fn project_row(
    metadata: &DatabaseMetadata,
    row: &QueryRow,
    projections: &[BoundProjection],
) -> Result<Vec<Value>, ExecutorError> {
    let mut results = Vec::new();

    for projection in projections {
        match projection {
            BoundProjection::All => {
                for table_row in &row.table_rows {
                    results.extend(table_row.values.clone());
                }
            }
            BoundProjection::Column(column) => {
                let value = column_value(metadata, row, column)?;
                results.push(value.clone());
            }
            BoundProjection::Aggregate(_) => return Err(ExecutorError::Unsupported),
        }
    }

    Ok(results)
}
