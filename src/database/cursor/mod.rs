use crate::{
    database::{Database, DatabaseError, cursor::state::InputState},
    executor::state::select::{SelectState, SelectStep},
    tuple::Value,
};

pub mod state;

pub struct SelectCursor<'a> {
    database: &'a mut Database,
    input: InputState,
    state: Box<SelectState>,
}

impl<'a> SelectCursor<'a> {
    pub(super) fn new(database: &'a mut Database, input: InputState, state: SelectState) -> Self {
        Self {
            database,
            input,
            state: Box::new(state),
        }
    }

    pub fn next_row(&mut self) -> Result<Option<Vec<Value>>, DatabaseError> {
        loop {
            if self.state.is_done() {
                return Ok(None);
            }

            let step = if let Some(row) = self.input.next_input_row(self.database)? {
                self.state
                    .push_row(self.database.catalog.metadata(), &row)?
            } else {
                self.state.finish(self.database.catalog.metadata())?
            };

            return match step {
                SelectStep::Row(values) => Ok(Some(values)),
                SelectStep::NeedInput => continue,
                SelectStep::Done => Ok(None),
            };
        }
    }
}
