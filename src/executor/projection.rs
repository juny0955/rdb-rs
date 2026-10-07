use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::BoundProjection,
    tuple::Value,
};

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
}
