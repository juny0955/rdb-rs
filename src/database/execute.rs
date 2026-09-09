use crate::{
    binder::{BoundDelete, BoundInsert, BoundSelect, BoundUpdate},
    database::{Database, DatabaseError, ExecuteResult, search_index_row_ids},
    executor::Executor,
};

impl Database {
    pub(super) fn execute_insert(
        &mut self,
        bound: &BoundInsert,
    ) -> Result<ExecuteResult, DatabaseError> {
        let heap_table = self.table_cache.get_or_open_table(
            &mut self.storage_manager,
            &self.data_dir,
            bound.table_id,
        )?;

        let executor = Executor::new(&self.metadata);
        let result = executor.execute_insert(bound, heap_table, &mut self.storage_manager)?;
        self.insert_row_into_indexes(bound.table_id, result.row_id, &result.values)?;
        Ok(ExecuteResult::Command { affected_rows: 1 })
    }

    pub(super) fn execute_select(
        &mut self,
        bound: &BoundSelect,
    ) -> Result<ExecuteResult, DatabaseError> {
        let heap_table = self.table_cache.get_or_open_table(
            &mut self.storage_manager,
            &self.data_dir,
            bound.table_id,
        )?;

        let executor = Executor::new(&self.metadata);

        if let Some(row_ids) = search_index_row_ids(
            &self.metadata,
            &mut self.storage_manager,
            &self.data_dir,
            bound,
        )? {
            let mut rows = Vec::new();
            for row_id in row_ids {
                let row = heap_table.get(row_id, &mut self.storage_manager)?;
                rows.push((row_id, row));
            }

            let results = executor.projection_and_filtered_rows(rows, bound)?;
            return Ok(ExecuteResult::Rows(results));
        }

        let results = executor.execute_select(bound, heap_table, &mut self.storage_manager)?;
        Ok(ExecuteResult::Rows(results))
    }

    pub(super) fn execute_update(
        &mut self,
        bound: &BoundUpdate,
    ) -> Result<ExecuteResult, DatabaseError> {
        let heap_table = self.table_cache.get_or_open_table(
            &mut self.storage_manager,
            &self.data_dir,
            bound.table_id,
        )?;
        let executor = Executor::new(&self.metadata);
        let results = executor.execute_update(bound, heap_table, &mut self.storage_manager)?;
        let affected_rows = results.len();

        for result in results {
            self.delete_row_from_indexes(bound.table_id, result.row_id, &result.old_values)?;
            self.insert_row_into_indexes(bound.table_id, result.row_id, &result.new_values)?;
        }
        Ok(ExecuteResult::Command { affected_rows })
    }

    pub(super) fn execute_delete(
        &mut self,
        bound: &BoundDelete,
    ) -> Result<ExecuteResult, DatabaseError> {
        let heap_table = self.table_cache.get_or_open_table(
            &mut self.storage_manager,
            &self.data_dir,
            bound.table_id,
        )?;
        let executor = Executor::new(&self.metadata);
        let results = executor.execute_delete(bound, heap_table, &mut self.storage_manager)?;
        let affected_rows = results.len();

        for result in results {
            self.delete_row_from_indexes(bound.table_id, result.row_id, &result.values)?;
        }

        Ok(ExecuteResult::Command { affected_rows })
    }
}
