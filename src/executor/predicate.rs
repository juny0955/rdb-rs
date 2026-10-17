use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::BoundExpression,
};

impl<'a> Executor<'a> {
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
