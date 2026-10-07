use std::collections::HashMap;

use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::{BoundColumnReference, BoundJoinCondition},
    tuple::Value,
};

impl<'a> Executor<'a> {
    pub fn hash_join(
        &self,
        left_rows: &[QueryRow],
        right_rows: &[QueryRow],
        condition: &BoundJoinCondition,
    ) -> Result<Vec<QueryRow>, ExecutorError> {
        let mut hash: HashMap<Value, Vec<usize>> = HashMap::new();
        let mut joined_rows = Vec::new();

        for (index, right_row) in right_rows.iter().enumerate() {
            let Some(column) = self.hash_join_column(right_row, condition) else {
                return self.nested_loop_join(left_rows, right_rows, condition);
            };

            let value = self.column_value(right_row, column)?;
            if *value != Value::Null {
                hash.entry(value.clone()).or_default().push(index);
            }
        }

        for left_row in left_rows {
            let Some(column) = self.hash_join_column(left_row, condition) else {
                return self.nested_loop_join(left_rows, right_rows, condition);
            };

            if let Some(right_indexes) = hash.get(self.column_value(left_row, column)?) {
                for right_index in right_indexes {
                    let mut table_rows = left_row.table_rows.clone();
                    table_rows.extend(right_rows[*right_index].table_rows.iter().cloned());
                    joined_rows.push(QueryRow { table_rows });
                }
            }
        }

        Ok(joined_rows)
    }

    fn hash_join_column<'row>(
        &self,
        row: &'row QueryRow,
        condition: &'row BoundJoinCondition,
    ) -> Option<&'row BoundColumnReference> {
        if row.find_table_row(condition.left.instance_id).is_some() {
            Some(&condition.left)
        } else if row.find_table_row(condition.right.instance_id).is_some() {
            Some(&condition.right)
        } else {
            None
        }
    }
}
