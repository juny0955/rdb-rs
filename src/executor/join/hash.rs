use std::collections::HashMap;

use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::{BoundColumnReference, BoundJoinCondition},
    tuple::Value,
};

pub struct HashJoinState {
    buckets: HashMap<Value, Vec<QueryRow>>,
    current_left: Option<QueryRow>,
    current_key: Option<Value>,
    match_index: usize,
}

impl HashJoinState {
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            current_left: None,
            current_key: None,
            match_index: 0,
        }
    }

    pub fn push_right(
        &mut self,
        executor: &Executor<'_>,
        row: QueryRow,
        condition: &BoundJoinCondition,
    ) -> Result<(), ExecutorError> {
        let column_ref = executor.hash_join_column(&row, condition)?;

        let value = executor.column_value(&row, column_ref)?;
        if *value != Value::Null {
            self.buckets.entry(value.clone()).or_default().push(row);
        }

        Ok(())
    }

    pub fn push_left(
        &mut self,
        executor: &Executor<'_>,
        row: QueryRow,
        condition: &BoundJoinCondition,
    ) -> Result<(), ExecutorError> {
        self.init_left();

        let column_ref = executor.hash_join_column(&row, condition)?;

        let value = executor.column_value(&row, column_ref)?;
        if *value == Value::Null || !self.buckets.contains_key(value) {
            return Ok(());
        }

        self.current_key = Some(value.clone());
        self.current_left = Some(row);

        Ok(())
    }

    pub fn next_joined_row(&mut self) -> Option<QueryRow> {
        let left_row = self.current_left.as_ref()?;
        let left_key = self.current_key.as_ref()?;

        let Some(right_rows) = self.buckets.get(left_key) else {
            self.init_left();
            return None;
        };

        let Some(right_row) = right_rows.get(self.match_index) else {
            self.init_left();
            return None;
        };

        let mut table_rows = left_row.table_rows.clone();
        table_rows.extend(right_row.table_rows.iter().cloned());

        self.match_index += 1;
        Some(QueryRow::new(table_rows))
    }

    pub fn rescan(&mut self) {
        self.init_left();
    }

    fn init_left(&mut self) {
        self.current_key = None;
        self.current_left = None;
        self.match_index = 0;
    }
}

impl<'a> Executor<'a> {
    fn hash_join_column<'row>(
        &self,
        row: &'row QueryRow,
        condition: &'row BoundJoinCondition,
    ) -> Result<&'row BoundColumnReference, ExecutorError> {
        if row.find_table_row(condition.left.instance_id).is_some() {
            Ok(&condition.left)
        } else if row.find_table_row(condition.right.instance_id).is_some() {
            Ok(&condition.right)
        } else {
            Err(ExecutorError::Unsupported)
        }
    }
}

#[cfg(test)]
#[path = "../tests/hash_join_state.rs"]
mod tests;
