use crate::{
    database::{
        Database, DatabaseError,
        cursor::state::{
            index::IndexScanState,
            join::{JoinScanState, hash::HashJoin, nested_loop::NestedLoopJoin},
            scan::ScanState,
        },
    },
    executor::QueryRow,
    index::IndexManager,
    query::{
        binder::{
            BoundColumnReference, BoundExpression, BoundFromClause, BoundJoinCondition, BoundTable,
            TableInstanceId,
        },
        common::ComparisonOperator,
    },
    tuple::Value,
};

mod index;
mod join;
mod scan;

pub enum InputState {
    SeqScan(ScanState),
    IndexScan(IndexScanState),
    Join(Box<dyn JoinScanState>),
}

impl InputState {
    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        match self {
            Self::SeqScan(state) => state.next_input_row(database),
            Self::IndexScan(state) => state.next_input_row(database),
            Self::Join(state) => state.next_input_row(database),
        }
    }

    pub fn from_clause(
        database: &mut Database,
        from: &BoundFromClause,
        filter: Option<&BoundExpression>,
    ) -> Result<Self, DatabaseError> {
        match from {
            BoundFromClause::InnerJoin { left, on, right } => {
                let can_hash_join = Self::can_hash_join(left, right, on);

                let left = Self::from_clause(database, left, filter)?;
                let right = Self::from_clause(database, right, filter)?;

                let join_state: Box<dyn JoinScanState> = if can_hash_join {
                    Box::new(HashJoin::new(left, right, on.clone()))
                } else {
                    Box::new(NestedLoopJoin::new(left, right, on.clone()))
                };

                Ok(Self::Join(join_state))
            }
            BoundFromClause::Table(table) => {
                if let Some((column, value)) = index_predicate_for_table(filter, table)
                    && let Some(row_ids) = IndexManager::search_index_row_ids(
                        &mut database.storage_manager,
                        database.catalog.metadata().indexes(),
                        table.table_id,
                        column.column_id,
                        value,
                    )?
                {
                    let index_state = IndexScanState::new(table.clone(), row_ids);
                    return Ok(Self::IndexScan(index_state));
                }

                let page_count = database
                    .table_manager
                    .page_count(&mut database.storage_manager, table.table_id)?;
                let scan_state = ScanState::new(table.clone(), page_count);
                Ok(Self::SeqScan(scan_state))
            }
        }
    }

    pub fn rescan(&mut self) {
        match self {
            Self::SeqScan(state) => state.rescan(),
            Self::IndexScan(state) => state.rescan(),
            Self::Join(state) => state.rescan(),
        }
    }

    fn can_hash_join(
        left: &BoundFromClause,
        right: &BoundFromClause,
        condition: &BoundJoinCondition,
    ) -> bool {
        // 정방향 ex) users u join orders o on u.id = o.user_id
        let normal = Self::contains_instance(left, condition.left.instance_id)
            && Self::contains_instance(right, condition.right.instance_id);

        // 역방향 ex) users u join orders o on o.user_id = u.id
        let reverse = Self::contains_instance(left, condition.right.instance_id)
            && Self::contains_instance(right, condition.left.instance_id);

        normal || reverse
    }

    fn contains_instance(from: &BoundFromClause, instance_id: TableInstanceId) -> bool {
        match from {
            BoundFromClause::InnerJoin { left, right, .. } => {
                Self::contains_instance(left, instance_id)
                    || Self::contains_instance(right, instance_id)
            }
            BoundFromClause::Table(table) => table.instance_id == instance_id,
        }
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
