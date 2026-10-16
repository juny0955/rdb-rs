use crate::{
    database::{Database, DatabaseError},
    executor::QueryRow,
};

pub mod hash;
pub mod nested_loop;

pub trait JoinScanState {
    fn next_input_row(
        &mut self,
        database: &mut Database,
    ) -> Result<Option<QueryRow>, DatabaseError>;

    fn rescan(&mut self);

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any;
}
