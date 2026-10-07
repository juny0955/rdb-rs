use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::{
        binder::{BoundColumnReference, BoundJoinCondition},
        common::ComparisonOperator,
    },
    tuple::Value,
};

impl<'a> Executor<'a> {
    pub fn nested_loop_join(
        &self,
        left_rows: &[QueryRow],
        right_rows: &[QueryRow],
        condition: &BoundJoinCondition,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut joined_rows = Vec::new();

        for left_row in left_rows {
            for right_row in right_rows {
                if self.matches_join_condition(left_row, right_row, condition)? {
                    let mut table_rows = left_row.table_rows.clone();
                    table_rows.extend(right_row.table_rows.iter().cloned());
                    joined_rows.push(QueryRow::new(table_rows));
                }
            }
        }

        Ok(joined_rows)
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
