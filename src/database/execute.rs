use crate::{
    database::{
        Database, DatabaseError, ExecuteResult,
        cursor::{
            SelectCursor,
            state::{CursorState, InputState},
        },
    },
    executor::{Executor, QueryRow},
    index::IndexManager,
    query::binder::{
        BoundDelete, BoundExpression, BoundFromClause, BoundInsert, BoundSelect, BoundUpdate,
    },
};

impl Database {
    pub(super) fn execute_insert(
        &mut self,
        bound: BoundInsert,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        let executor = Executor::new(self.catalog.metadata());
        let row = executor.encode_insert(&bound)?;

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

        Ok(ExecuteResult::Success)
    }

    pub(super) fn execute_select(
        &mut self,
        bound: BoundSelect,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        if bound.order_by.is_none() {
            let input = InputState::from_clause(self, &bound.from, bound.filter.as_ref())?;
            let state = CursorState::new(bound, input)?;

            return Ok(ExecuteResult::Rows(Box::new(SelectCursor::new(
                self, state,
            ))));
        }

        let rows = self.execute_from_clause(&bound.from, &bound.filter)?;
        let executor = Executor::new(self.catalog.metadata());
        let results = executor.select_rows(rows, &bound)?;

        Ok(ExecuteResult::Rows(Box::new(SelectCursor::new(
            self,
            CursorState::Buffered(results.into_iter()),
        ))))
    }

    pub(super) fn execute_update(
        &mut self,
        bound: BoundUpdate,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        let rows = self.execute_from_clause(&bound.from, &bound.filter)?;

        let executor = Executor::new(self.catalog.metadata());
        let prepared_updates = executor.prepare_update(rows, &bound)?;
        let affected_rows = prepared_updates.len();

        for prepared_update in prepared_updates {
            IndexManager::update_row(
                &mut self.storage_manager,
                &self.catalog,
                prepared_update.table_id,
                prepared_update.row_id,
                &prepared_update.old_values,
                &prepared_update.new_values,
            )?;

            self.table_manager.update_row(
                &mut self.storage_manager,
                prepared_update.table_id,
                prepared_update.row_id,
                &prepared_update.new_row,
            )?;
        }

        Ok(ExecuteResult::AffectedRows { affected_rows })
    }

    pub(super) fn execute_delete(
        &mut self,
        bound: BoundDelete,
    ) -> Result<ExecuteResult<'_>, DatabaseError> {
        let rows = self.execute_from_clause(&bound.from, &bound.filter)?;

        let executor = Executor::new(self.catalog.metadata());
        let prepared_deletes = executor.prepare_delete(rows, &bound)?;
        let affected_rows = prepared_deletes.len();

        for prepared_delete in prepared_deletes {
            IndexManager::delete_row(
                &mut self.storage_manager,
                &self.catalog,
                prepared_delete.table_id,
                prepared_delete.row_id,
                &prepared_delete.values,
            )?;

            self.table_manager.delete_row(
                &mut self.storage_manager,
                prepared_delete.table_id,
                prepared_delete.row_id,
            )?;
        }

        Ok(ExecuteResult::AffectedRows { affected_rows })
    }

    fn execute_from_clause(
        &mut self,
        from: &BoundFromClause,
        filter: &Option<BoundExpression>,
    ) -> Result<Vec<QueryRow>, DatabaseError> {
        let mut input = InputState::from_clause(self, from, filter.as_ref())?;

        let mut results = Vec::new();
        while let Some(row) = input.next_input_row(self)? {
            results.push(row);
        }

        Ok(results)
    }
}
