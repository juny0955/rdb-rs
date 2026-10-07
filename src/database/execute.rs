use crate::{
    database::{
        Database, DatabaseError, ExecuteResult,
        select_cursor::{SelectCursor, SelectSource},
    },
    executor::{Executor, QueryRow, TableRow},
    index::IndexManager,
    query::{
        binder::{
            BoundColumnReference, BoundDelete, BoundExpression, BoundFromClause, BoundInsert,
            BoundSelect, BoundTable, BoundUpdate,
        },
        common::ComparisonOperator,
    },
    storage::page::RowId,
    tuple::Value,
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
        if let BoundFromClause::Table(table) = &bound.from
            && bound.group_by.is_none()
            && bound.order_by.is_none()
            && index_predicate_for_table(bound.filter.as_ref(), table).is_none()
        {
            let page_count = self
                .table_manager
                .page_count(&mut self.storage_manager, table.table_id)?;

            let source = SelectSource::new_scan(bound, page_count)?;
            return Ok(ExecuteResult::Rows(Box::new(SelectCursor::new(
                self, source,
            ))));
        }

        let rows = self.execute_from_clause(&bound.from, &bound.filter)?;

        let executor = Executor::new(self.catalog.metadata());
        let results = executor.select_rows(rows, &bound)?;

        Ok(ExecuteResult::Rows(Box::new(SelectCursor::new(
            self,
            SelectSource::Buffered(results.into_iter()),
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

        Ok(ExecuteResult::Command { affected_rows })
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

        Ok(ExecuteResult::Command { affected_rows })
    }

    fn execute_from_clause(
        &mut self,
        from: &BoundFromClause,
        filter: &Option<BoundExpression>,
    ) -> Result<Vec<QueryRow>, DatabaseError> {
        match from {
            BoundFromClause::InnerJoin { left, on, right } => {
                let left_rows = self.execute_from_clause(left, filter)?;
                let right_rows = self.execute_from_clause(right, filter)?;
                let executor = Executor::new(self.catalog.metadata());
                Ok(executor.hash_join(&left_rows, &right_rows, on)?)
            }
            BoundFromClause::Table(table) => {
                let table_rows = self.fetch_table_rows(table, filter)?;

                Ok(table_rows
                    .into_iter()
                    .map(|table_row| QueryRow {
                        table_rows: vec![table_row],
                    })
                    .collect())
            }
        }
    }

    fn fetch_table_rows(
        &mut self,
        table: &BoundTable,
        filter: &Option<BoundExpression>,
    ) -> Result<Vec<TableRow>, DatabaseError> {
        if let Some((column, value)) = index_predicate_for_table(filter.as_ref(), table)
            && let Some(row_ids) = IndexManager::search_index_row_ids(
                &mut self.storage_manager,
                self.catalog.metadata().indexes(),
                table.table_id,
                column.column_id,
                value,
            )?
        {
            return self.get_table_rows(table, row_ids);
        }

        self.scan_table_rows(table)
    }

    fn get_table_rows(
        &mut self,
        table: &BoundTable,
        row_ids: Vec<RowId>,
    ) -> Result<Vec<TableRow>, DatabaseError> {
        let rows =
            self.table_manager
                .get_rows(&mut self.storage_manager, table.table_id, row_ids)?;

        let executor = Executor::new(self.catalog.metadata());
        Ok(executor.decode_table_rows(table, rows)?)
    }

    fn scan_table_rows(&mut self, table: &BoundTable) -> Result<Vec<TableRow>, DatabaseError> {
        let rows = self
            .table_manager
            .scan_rows(&mut self.storage_manager, table.table_id)?;

        let executor = Executor::new(self.catalog.metadata());
        Ok(executor.decode_table_rows(table, rows)?)
    }
}

fn index_predicate_for_table<'a>(
    filter: Option<&'a BoundExpression>,
    table: &BoundTable,
) -> Option<(&'a BoundColumnReference, &'a Value)> {
    match filter? {
        BoundExpression::Comparison {
            column,
            operator,
            value,
        } => {
            if *operator == ComparisonOperator::Equal && column.instance_id == table.instance_id {
                Some((column, value))
            } else {
                None
            }
        }
        BoundExpression::And { left, right } => {
            index_predicate_for_table(Some(left.as_ref()), table)
                .or_else(|| index_predicate_for_table(Some(right.as_ref()), table))
        }
        BoundExpression::Or { .. } => None,
    }
}
