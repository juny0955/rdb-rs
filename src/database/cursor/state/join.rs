use crate::{
    database::{Database, DatabaseError, cursor::state::InputState},
    executor::{Executor, QueryRow, join::hash::HashJoinState},
    query::binder::BoundJoinCondition,
};

pub struct JoinScanState {
    left: Box<InputState>,
    right: Box<InputState>,
    condition: BoundJoinCondition,
    join: HashJoinState,
    right_built: bool,
}

impl JoinScanState {
    pub fn new(left: InputState, right: InputState, condition: BoundJoinCondition) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            join: HashJoinState::new(),
            right_built: false,
            condition,
        }
    }

    pub fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError> {
        self.build_right(database)?;

        loop {
            if let Some(row) = self.join.next_joined_row() {
                return Ok(Some(row));
            }

            let Some(left_row) = self.left.next_input_row(database)? else {
                return Ok(None);
            };

            let executor = Executor::new(database.catalog.metadata());
            self.join.push_left(&executor, left_row, &self.condition)?;
        }
    }

    fn build_right(&mut self, database: &mut Database) -> Result<(), DatabaseError> {
        if self.right_built {
            return Ok(());
        }

        loop {
            let Some(row) = self.right.next_input_row(database)? else {
                self.right_built = true;
                return Ok(());
            };

            let executor = Executor::new(database.catalog.metadata());
            self.join.push_right(&executor, row, &self.condition)?;
        }
    }
}
