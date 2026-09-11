use crate::{
    binder::{BoundDelete, BoundExpression, BoundInsert, BoundSelect, BoundUpdate},
    database::{Database, DatabaseError, ExecuteResult},
    executor::Executor,
    index::IndexManager,
};

impl Database {
    pub(super) fn execute_insert(
        &mut self,
        bound: &BoundInsert,
    ) -> Result<ExecuteResult, DatabaseError> {
        let executor = Executor::new(self.catalog.metadata());
        let row = executor.encode_insert(bound)?;
        let row_id =
            self.table_manager
                .insert_row(&mut self.storage_manager, bound.table_id, &row)?;

        IndexManager::insert_row(
            &mut self.storage_manager,
            &self.catalog,
            bound.table_id,
            row_id,
            &bound.values,
        )?;

        Ok(ExecuteResult::Command { affected_rows: 1 })
    }

    pub(super) fn execute_select(
        &mut self,
        bound: &BoundSelect,
    ) -> Result<ExecuteResult, DatabaseError> {
        let indexes = self.catalog.metadata().indexes();
        let index_scan = match &bound.filter {
            Some(BoundExpression::Equal { column_id, value }) => {
                IndexManager::search_index_row_ids(
                    &mut self.storage_manager,
                    indexes,
                    bound.table_id,
                    *column_id,
                    value,
                )?
            }
            None => None,
        };

        let rows = if let Some(row_ids) = index_scan {
            self.table_manager
                .get_rows(&mut self.storage_manager, bound.table_id, row_ids)?
        } else {
            self.table_manager
                .scan_rows(&mut self.storage_manager, bound.table_id)?
        };

        let executor = Executor::new(self.catalog.metadata());
        let results = executor.projection_and_filtered_rows(rows, bound)?;
        Ok(ExecuteResult::Rows(results))
    }

    pub(super) fn execute_update(
        &mut self,
        bound: &BoundUpdate,
    ) -> Result<ExecuteResult, DatabaseError> {
        let executor = Executor::new(self.catalog.metadata());

        let rows = self
            .table_manager
            .scan_rows(&mut self.storage_manager, bound.table_id)?;
        let prepared_updates = executor.prepare_update(bound, rows)?;
        let affected_rows = prepared_updates.len();

        for prepared_update in &prepared_updates {
            self.table_manager.update_row(
                &mut self.storage_manager,
                bound.table_id,
                prepared_update.row_id,
                &prepared_update.new_row,
            )?;
        }

        for prepared_update in prepared_updates {
            IndexManager::delete_row(
                &mut self.storage_manager,
                &self.catalog,
                bound.table_id,
                prepared_update.row_id,
                &prepared_update.old_values,
            )?;

            IndexManager::insert_row(
                &mut self.storage_manager,
                &self.catalog,
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
        let executor = Executor::new(self.catalog.metadata());

        let rows = self
            .table_manager
            .scan_rows(&mut self.storage_manager, bound.table_id)?;
        let prepared_deletes = executor.prepare_delete(bound, rows)?;
        let affected_rows = prepared_deletes.len();

        for prepared_delete in &prepared_deletes {
            self.table_manager.delete_row(
                &mut self.storage_manager,
                bound.table_id,
                prepared_delete.row_id,
            )?;
        }

        for prepared_delete in prepared_deletes {
            IndexManager::delete_row(
                &mut self.storage_manager,
                &self.catalog,
                bound.table_id,
                prepared_delete.row_id,
                &prepared_delete.values,
            )?;
        }

        Ok(ExecuteResult::Command { affected_rows })
    }
}
