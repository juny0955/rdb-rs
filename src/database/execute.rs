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
        let row = executor.encode_insert(bound)?;
        let row_id = heap_table.insert(&row, &mut self.storage_manager)?;
        self.insert_row_into_indexes(bound.table_id, row_id, &bound.values)?;
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

        let index_scan = search_index_row_ids(
            &self.metadata,
            &mut self.storage_manager,
            &self.data_dir,
            bound,
        )?;

        let rows = if let Some(row_ids) = index_scan {
            let mut rows = Vec::new();
            for row_id in row_ids {
                let row = heap_table.get(row_id, &mut self.storage_manager)?;
                rows.push((row_id, row));
            }

            rows
        } else {
            heap_table.scan(&mut self.storage_manager)?
        };

        let results = executor.projection_and_filtered_rows(rows, bound)?;
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

        let rows = heap_table.scan(&mut self.storage_manager)?;
        let prepared_updates = executor.prepare_update(bound, rows)?;
        let affected_rows = prepared_updates.len();

        for prepared_update in &prepared_updates {
            heap_table.update(
                prepared_update.row_id,
                &prepared_update.new_row,
                &mut self.storage_manager,
            )?;
        }

        for prepared_update in prepared_updates {
            self.delete_row_from_indexes(
                bound.table_id,
                prepared_update.row_id,
                &prepared_update.old_values,
            )?;
            self.insert_row_into_indexes(
                bound.table_id,
                prepared_update.row_id,
                &prepared_update.new_values,
            )?;
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

        let rows = heap_table.scan(&mut self.storage_manager)?;
        let prepared_deletes = executor.prepare_delete(bound, rows)?;
        let affected_rows = prepared_deletes.len();

        for prepared_delete in &prepared_deletes {
            heap_table.delete(prepared_delete.row_id, &mut self.storage_manager)?;
        }

        for prepared_delete in prepared_deletes {
            self.delete_row_from_indexes(
                bound.table_id,
                prepared_delete.row_id,
                &prepared_delete.values,
            )?;
        }

        Ok(ExecuteResult::Command { affected_rows })
    }
}
