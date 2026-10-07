use std::collections::HashMap;

use crate::{
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::BoundColumnReference,
    tuple::Value,
};

pub(super) struct RowGroup {
    pub key: Vec<Value>,
    pub rows: Vec<QueryRow>,
}

impl<'a> Executor<'a> {
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
}
