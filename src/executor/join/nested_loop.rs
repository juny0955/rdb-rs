use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::{
        binder::{BoundColumnReference, BoundJoinCondition},
        common::ComparisonOperator,
    },
    tuple::Value,
};

impl<'a> Executor<'a> {
    pub fn join_row_pair(
        &self,
        left: &QueryRow,
        right: &QueryRow,
        condition: &BoundJoinCondition,
    ) -> Result<Option<QueryRow>, ExecutorError> {
        if self.matches_join_condition(left, right, condition)? {
            let mut table_rows = left.table_rows.clone();
            table_rows.extend(right.table_rows.iter().cloned());
            return Ok(Some(QueryRow::new(table_rows)));
        }

        Ok(None)
    }

    fn matches_join_condition(
        &self,
        left_row: &QueryRow,
        right_row: &QueryRow,
        condition: &BoundJoinCondition,
    ) -> Result<bool, ExecutorError> {
        let left_value = self.join_column_value(left_row, right_row, &condition.left)?;
        let right_value = self.join_column_value(left_row, right_row, &condition.right)?;

        Ok(left_value.compare(ComparisonOperator::Equal, right_value))
    }

    fn join_column_value<'row>(
        &self,
        left_row: &'row QueryRow,
        right_row: &'row QueryRow,
        column: &BoundColumnReference,
    ) -> Result<&'row Value, ExecutorError> {
        let source = if left_row.find_table_row(column.instance_id).is_some() {
            left_row
        } else if right_row.find_table_row(column.instance_id).is_some() {
            right_row
        } else {
            return Err(ExecutorError::TableInstanceNotFound(column.instance_id));
        };

        self.column_value(source, column)
    }
}
