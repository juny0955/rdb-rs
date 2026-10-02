use std::collections::HashMap;

use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::{BoundAggregate, BoundColumnReference, BoundProjection},
    tuple::Value,
};

pub(super) struct RowGroup {
    pub key: Vec<Value>,
    pub rows: Vec<QueryRow>,
}

impl<'a> Executor<'a> {
    pub(crate) fn project_row(
        &self,
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
                    let value = self.column_value(row, column)?;
                    results.push(value.clone());
                }
                BoundProjection::Aggregate(_) => return Err(ExecutorError::Unsupported),
            }
        }

        Ok(results)
    }

    pub(super) fn project_rows(
        &self,
        rows: &[QueryRow],
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();
        for row in rows {
            let values = self.project_row(row, projections)?;
            results.push(values);
        }

        Ok(results)
    }

    pub(super) fn group_rows(
        &self,
        rows: Vec<QueryRow>,
        group_by: &[BoundColumnReference],
    ) -> Result<Vec<RowGroup>, ExecutorError> {
        let mut groups: Vec<RowGroup> = Vec::new();
        let mut indices: HashMap<Vec<Value>, usize> = HashMap::new();
        for row in rows {
            let mut key = Vec::new();
            for column_ref in group_by {
                let value = self.column_value(&row, column_ref)?;
                key.push(value.clone());
            }

            if let Some(&group_index) = indices.get(&key) {
                groups[group_index].rows.push(row);
            } else {
                indices.insert(key.clone(), groups.len());
                groups.push(RowGroup {
                    key,
                    rows: vec![row],
                });
            }
        }

        Ok(groups)
    }

    pub(super) fn aggregate(
        &self,
        rows: Vec<QueryRow>,
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let group = RowGroup { key: vec![], rows };
        self.aggregate_groups(vec![group], &[], projections)
    }

    pub(super) fn aggregate_groups(
        &self,
        groups: Vec<RowGroup>,
        group_by: &[BoundColumnReference],
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();
        for group in groups {
            results.push(self.aggregate_group(&group, group_by, projections)?);
        }

        Ok(results)
    }

    fn aggregate_group(
        &self,
        group: &RowGroup,
        group_by: &[BoundColumnReference],
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
                BoundProjection::Aggregate(BoundAggregate::Sum(column)) => {
                    let mut sum: Option<i64> = None;
                    for row in &group.rows {
                        let value = self.column_value(row, column)?;
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
}

fn value_to_addend(value: &Value) -> Result<Option<i64>, ExecutorError> {
    Ok(match value {
        Value::Int(v) => Some(*v as i64),
        Value::BigInt(v) => Some(*v),
        Value::Null => None,
        _ => return Err(ExecutorError::Unsupported),
    })
}
