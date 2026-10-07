use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::BoundExpression,
};

impl<'a> Executor<'a> {
    pub(super) fn filter_rows(
        &self,
        rows: Vec<QueryRow>,
        filter: &BoundExpression,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut results = Vec::new();

        for row in rows {
            if self.row_matches_filter(&row, filter)? {
                results.push(row);
            }
        }

        Ok(results)
    }

    pub(crate) fn row_matches_filter(
        &self,
        row: &QueryRow,
        filter: &BoundExpression,
    ) -> Result<bool, ExecutorError> {
        Ok(match filter {
            BoundExpression::And { left, right } => {
                self.row_matches_filter(row, left)? && self.row_matches_filter(row, right)?
            }
            BoundExpression::Or { left, right } => {
                self.row_matches_filter(row, left)? || self.row_matches_filter(row, right)?
            }
            BoundExpression::Comparison {
                column,
                operator,
                value,
            } => {
                let column_value = self.column_value(row, column)?;
                column_value.compare(*operator, value)
            }
        })
    }
}
