use crate::{
    catalog::metadata::TableId, database::{
        Database, DatabaseError, ExecuteResult,
        cursor::{SelectCursor, state::InputState},
    }, executor::{
        Executor, QueryRow,
        state::{delete::DeleteState, select::SelectState},
    }, index::IndexManager, query::binder::{
        BoundDelete, BoundExpression, BoundFromClause, BoundInsert, BoundSelect, BoundUpdate,
    }, storage::page::RowId, tuple::{Value, decode}
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
        let input = InputState::from_clause(self, &bound.from, bound.filter.as_ref())?;

        Ok(ExecuteResult::Rows(Box::new(SelectCursor::new(
            self,
            input,
            SelectState::new(bound)?,
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
        let is_join = matches!(&bound.from, BoundFromClause::InnerJoin { .. });
        let mut input = InputState::from_clause(self, &bound.from, bound.filter.as_ref())?;
        let mut state = DeleteState::new(bound);
        let mut affected_rows = 0;

        // join인 경우 join에서 읽어야할 원본 행이 먼저 삭제되지 않도록
        // DeleteState.seen에 누적된 식별자들을 꺼내어 삭제한다
        if is_join {
            while let Some(row) = input.next_input_row(self)? {
                let executor = Executor::new(self.catalog.metadata());
                state.prepare_row(&executor, &row)?;
            }

            for (table_id, row_id) in state.into_targets() {
                if let Some(table_meta) = self.catalog.metadata().table_by_id(table_id) {
                    let row = self.table_manager.get_row(&mut self.storage_manager, table_id, row_id)?;
                    let values = decode(&row, table_meta.columns())?;

                    self.delete_target(table_id, row_id, &values)?;
                    affected_rows += 1;
                }
            }
        } else {
            while let Some(row) = input.next_input_row(self)? {
                let executor = Executor::new(self.catalog.metadata());
                let prepared_deletes = state.prepare_row(&executor, &row)?;

                for prepared_delete in prepared_deletes {
                    self.delete_target(prepared_delete.table_id, prepared_delete.row_id, &prepared_delete.values)?;
                    affected_rows += 1;
                }
            }
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

    fn delete_target(&mut self, table_id: TableId, row_id: RowId, values: &[Value]) -> Result<(), DatabaseError> {
        IndexManager::delete_row(
            &mut self.storage_manager,
            &self.catalog,
            table_id,
            row_id,
            values,
        )?;

        self.table_manager.delete_row(
            &mut self.storage_manager,
            table_id,
            row_id,
        )?;

        Ok(())
    }
}
