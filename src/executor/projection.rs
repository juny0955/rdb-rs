use crate::{
    binder::BoundProjection,
    catalog::metadata::TableMetadata,
    executor::ExecutorError,
    storage::page::{Row, RowId},
    tuple::{Value, decode},
};

pub(super) fn project_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    projections: &[BoundProjection],
) -> Result<Vec<Vec<Value>>, ExecutorError> {
    let mut results = Vec::new();

    for (_, row) in rows {
        let values = decode(&row, table.columns())?;
        let mut projection_values = Vec::new();

        for projection in projections {
            match projection {
                BoundProjection::All => projection_values.extend(values.iter().cloned()),
                BoundProjection::Column(column_id) => {
                    let column_index = table
                        .column_index(*column_id)
                        .ok_or(ExecutorError::ColumnNotFound(*column_id))?;
                    projection_values.push(values[column_index].clone());
                }
            }
        }

        results.push(projection_values);
    }

    Ok(results)
}
