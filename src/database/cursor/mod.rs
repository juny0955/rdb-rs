use crate::{
    database::{Database, DatabaseError, cursor::state::CursorState},
    executor::{Executor, select::SelectStep},
    tuple::Value,
};

pub mod state;

pub struct SelectCursor<'a> {
    database: &'a mut Database,
    state: CursorState,
}

impl<'a> SelectCursor<'a> {
    pub(super) fn new(database: &'a mut Database, state: CursorState) -> Self {
        Self { database, state }
    }

    pub fn next_row(&mut self) -> Result<Option<Vec<Value>>, DatabaseError> {
        match &mut self.state {
            CursorState::Scan { input, state } => loop {
                if state.is_done() {
                    return Ok(None);
                }

                let step = if let Some(row) = input.next_input_row(self.database)? {
                    let executor = Executor::new(self.database.catalog.metadata());
                    state.push_row(&executor, &row)?
                } else {
                    state.finish()?
                };

                return match step {
                    SelectStep::Row(values) => Ok(Some(values)),
                    SelectStep::NeedInput => continue,
                    SelectStep::Done => Ok(None),
                };
            },
            CursorState::Buffered(current) => {
                let next = current.next();
                if next.is_none() {
                    self.state = CursorState::Done;
                }

                Ok(next)
            }
            CursorState::Done => Ok(None),
        }
    }
}
