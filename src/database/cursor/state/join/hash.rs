use crate::{
    database::{
        Database, DatabaseError,
        cursor::state::{InputState, join::JoinScanState},
    },
    executor::{QueryRow, join::hash::HashJoinState},
    query::binder::BoundJoinCondition,
};

pub struct HashJoin {
    left: Box<InputState>,
    right: Box<InputState>,
    condition: BoundJoinCondition,
    join: HashJoinState,
    right_built: bool,
}

impl HashJoin {
    pub fn new(left: InputState, right: InputState, condition: BoundJoinCondition) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            join: HashJoinState::new(),
            right_built: false,
            condition,
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

            self.join
                .push_right(database.catalog.metadata(), row, &self.condition)?;
        }
    }
}

impl JoinScanState for HashJoin {
    fn next_input_row(
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

            self.join
                .push_left(database.catalog.metadata(), left_row, &self.condition)?;
        }
    }

    fn rescan(&mut self) {
        self.left.rescan();
        self.join.rescan();
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "../../../tests/select_cursor_join.rs"]
mod tests;
