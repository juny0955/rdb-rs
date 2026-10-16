use crate::{
    database::{
        Database, DatabaseError,
        cursor::state::{InputState, join::JoinScanState},
    },
    executor::{Executor, QueryRow},
    query::binder::BoundJoinCondition,
};

pub struct NestedLoopJoin {
    left: Box<InputState>,
    right: Box<InputState>,
    condition: BoundJoinCondition,
    current_left: Option<QueryRow>,
}

impl NestedLoopJoin {
    pub fn new(left: InputState, right: InputState, condition: BoundJoinCondition) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            condition,
            current_left: None,
        }
    }

    fn fetch_left(&mut self, database: &mut Database) -> Result<(), DatabaseError> {
        if self.current_left.is_some() {
            return Ok(());
        }

        self.current_left = self.left.next_input_row(database)?;

        if self.current_left.is_some() {
            self.right.rescan();
        }
        Ok(())
    }
}

impl JoinScanState for NestedLoopJoin {
    fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        loop {
            self.fetch_left(database)?;
            let Some(left) = self.current_left.as_ref() else {
                return Ok(None);
            };

            let Some(right) = self.right.next_input_row(database)? else {
                self.current_left = None;
                continue;
            };

            let executor = Executor::new(database.catalog.metadata());
            if let Some(row) = executor.join_row_pair(left, &right, &self.condition)? {
                return Ok(Some(row));
            }
        }
    }

    fn rescan(&mut self) {
        self.left.rescan();
        self.right.rescan();
        self.current_left = None;
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "../../../tests/nested_loop_cursor.rs"]
mod tests;
