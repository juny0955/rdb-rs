use crate::{
    catalog::metadata::{ColumnId, TableMetadata},
    executor::ExecutorError,
    query::binder::BoundProjection,
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
                BoundProjection::Aggregate(_) => return Err(ExecutorError::Unsupported),
            }
        }

        results.push(projection_values);
    }

    Ok(results)
}

pub(super) fn sum_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    column_id: ColumnId,
    limit: Option<usize>,
) -> Result<Vec<Vec<Value>>, ExecutorError> {
    let column_index = table
        .column_index(column_id)
        .ok_or(ExecutorError::ColumnNotFound(column_id))?;

    let mut sum: Option<i64> = None;
    for (_, row) in rows {
        let values = decode(&row, table.columns())?;
        let value = values[column_index].clone();
        let addend = match value {
            Value::Int(v) => v as i64,
            Value::BigInt(v) => v,
            Value::Null => continue,
            _ => return Err(ExecutorError::Unsupported),
        };

        let current = sum.unwrap_or(0);
        sum = Some(
            current
                .checked_add(addend)
                .ok_or(ExecutorError::SumOverflow)?,
        );
    }

    let result_value = sum.map(Value::BigInt).unwrap_or(Value::Null);
    let mut result = vec![vec![result_value]];
    if let Some(limit) = limit {
        result.truncate(limit);
    }

    Ok(result)
}
