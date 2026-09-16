use std::collections::HashMap;

use crate::{
    catalog::metadata::{ColumnId, TableMetadata},
    executor::ExecutorError,
    query::binder::{BoundAggregate, BoundProjection},
    storage::page::{Row, RowId},
    tuple::{Value, decode},
};

pub(super) struct RowGroup {
    pub key: Vec<Value>,
    pub rows: Vec<(RowId, Row)>,
}

pub(super) fn group_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    group_by: &[ColumnId],
) -> Result<Vec<RowGroup>, ExecutorError> {
    let mut column_indexes = Vec::new();
    for column_id in group_by {
        column_indexes.push(
            table
                .column_index(*column_id)
                .ok_or(ExecutorError::ColumnNotFound(*column_id))?,
        );
    }

    let mut groups: Vec<RowGroup> = Vec::new();
    let mut indices: HashMap<Vec<Value>, usize> = HashMap::new();
    for (row_id, row) in rows {
        let values = decode(&row, table.columns())?;

        let mut key = Vec::new();
        for column_index in &column_indexes {
            key.push(values[*column_index].clone());
        }

        if let Some(&group_index) = indices.get(&key) {
            groups[group_index].rows.push((row_id, row));
        } else {
            let group_index = groups.len();
            indices.insert(key.clone(), group_index);
            groups.push(RowGroup {
                key,
                rows: vec![(row_id, row)],
            });
        }
    }

    Ok(groups)
}

pub(super) fn aggregate_groups(
    groups: Vec<RowGroup>,
    group_by: &[ColumnId],
    table: &TableMetadata,
    projections: &[BoundProjection],
    limit: Option<usize>,
) -> Result<Vec<Vec<Value>>, ExecutorError> {
    let mut result = Vec::new();
    for group in groups {
        result.push(aggregate_group(&group, group_by, table, projections)?);
    }

    if let Some(limit) = limit {
        result.truncate(limit);
    }

    Ok(result)
}

pub(super) fn aggregate(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    projections: &[BoundProjection],
    limit: Option<usize>,
) -> Result<Vec<Vec<Value>>, ExecutorError> {
    let group = RowGroup { key: vec![], rows };
    aggregate_groups(vec![group], &[], table, projections, limit)
}

fn aggregate_group(
    group: &RowGroup,
    group_by: &[ColumnId],
    table: &TableMetadata,
    projections: &[BoundProjection],
) -> Result<Vec<Value>, ExecutorError> {
    let mut projection_result = Vec::new();
    for projection in projections {
        match projection {
            BoundProjection::Column(id) => {
                let key_index = group_by
                    .iter()
                    .position(|group_column_id| group_column_id == id)
                    .ok_or(ExecutorError::Unsupported)?;

                projection_result.push(group.key[key_index].clone());
            }
            BoundProjection::Aggregate(BoundAggregate::CountAll) => {
                let value = Value::BigInt(i64::try_from(group.rows.len()).map_err(|_| {
                    ExecutorError::CountOutOfRange {
                        count: group.rows.len(),
                    }
                })?);
                projection_result.push(value);
            }
            BoundProjection::Aggregate(BoundAggregate::Sum(id)) => {
                let column_index = table
                    .column_index(*id)
                    .ok_or(ExecutorError::ColumnNotFound(*id))?;

                let mut sum: Option<i64> = None;
                for (_, row) in &group.rows {
                    let values = decode(row, table.columns())?;
                    let value = values[column_index].clone();
                    if let Some(addend) = value_to_addend(value)? {
                        let current = sum.unwrap_or(0);
                        sum = Some(
                            current
                                .checked_add(addend)
                                .ok_or(ExecutorError::SumOverflow)?,
                        );
                    }
                }
                let value = sum.map(Value::BigInt).unwrap_or(Value::Null);
                projection_result.push(value);
            }
            _ => return Err(ExecutorError::Unsupported),
        }
    }

    Ok(projection_result)
}

pub(super) fn project_rows(
    rows: Vec<(RowId, Row)>,
    table: &TableMetadata,
    projections: &[BoundProjection],
    limit: Option<usize>,
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

    if let Some(limit) = limit {
        results.truncate(limit);
    }

    Ok(results)
}

fn value_to_addend(value: Value) -> Result<Option<i64>, ExecutorError> {
    Ok(match value {
        Value::Int(v) => Some(v as i64),
        Value::BigInt(v) => Some(v),
        Value::Null => None,
        _ => return Err(ExecutorError::Unsupported),
    })
}
