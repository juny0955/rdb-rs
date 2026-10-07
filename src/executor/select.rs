use crate::{
    executor::{Executor, ExecutorError, QueryRow, aggregate::AggregateState},
    query::binder::{BoundProjection, BoundSelect},
    tuple::Value,
};

pub enum SelectStep {
    NeedInput,
    Row(Vec<Value>),
    Done,
}

enum SelectMode {
    Rows,
    Aggregate(Vec<AggregateState>),
}

pub struct SelectState {
    bound: BoundSelect,
    mode: SelectMode,
    emitted: usize,
    done: bool,
}

impl SelectState {
    pub fn new(bound: BoundSelect) -> Result<Self, ExecutorError> {
        if bound.group_by.is_some() || bound.order_by.is_some() {
            return Err(ExecutorError::Unsupported);
        }

        let mode = if bound
            .projections
            .iter()
            .any(|projection| matches!(projection, BoundProjection::Aggregate(_)))
        {
            let mut states = Vec::new();
            for projection in &bound.projections {
                let BoundProjection::Aggregate(aggregate) = projection else {
                    return Err(ExecutorError::Unsupported);
                };
                states.push(AggregateState::new(aggregate));
            }
            SelectMode::Aggregate(states)
        } else {
            SelectMode::Rows
        };

        let mut done = false;
        if let Some(limit) = bound.limit
            && limit == 0
        {
            done = true;
        }

        Ok(Self {
            bound,
            mode,
            done,
            emitted: 0,
        })
    }

    pub fn push_row(
        &mut self,
        executor: &Executor<'_>,
        row: &QueryRow,
    ) -> Result<SelectStep, ExecutorError> {
        if self.is_done() {
            return Ok(SelectStep::Done);
        }

        if let Some(filter) = &self.bound.filter
            && !executor.row_matches_filter(row, filter)?
        {
            return Ok(SelectStep::NeedInput);
        }

        match &mut self.mode {
            SelectMode::Rows => {
                let values = executor.project_row(row, &self.bound.projections)?;
                self.emitted += 1;

                if let Some(limit) = self.bound.limit
                    && limit <= self.emitted
                {
                    self.done = true;
                }

                Ok(SelectStep::Row(values))
            }
            SelectMode::Aggregate(states) => {
                for (state, projection) in states.iter_mut().zip(&self.bound.projections) {
                    executor.accumulate_aggregate(state, projection, row)?;
                }

                Ok(SelectStep::NeedInput)
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn finish(&mut self) -> Result<SelectStep, ExecutorError> {
        if self.is_done() {
            return Ok(SelectStep::Done);
        }

        match &self.mode {
            SelectMode::Rows => {
                self.done = true;
                Ok(SelectStep::Done)
            }
            SelectMode::Aggregate(states) => {
                let mut row = Vec::new();
                for state in states {
                    row.push(state.result()?);
                }
                self.done = true;
                Ok(SelectStep::Row(row))
            }
        }
    }
}
