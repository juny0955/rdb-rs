use std::vec::IntoIter;

use crate::{
    catalog::metadata::DatabaseMetadata,
    executor::{
        ExecutorError, QueryRow,
        aggregate::{AggregateState, accumulate_aggregate},
        group::GroupState,
        order::order_rows,
        predicate::row_matches_filter,
        projection::project_row,
    },
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
    Group(GroupState),
    GroupOutput(IntoIter<Vec<Value>>),
    Sort(Vec<QueryRow>),
    SortOutput(IntoIter<QueryRow>),
}

impl SelectMode {
    pub fn new(bound: &BoundSelect) -> Result<Self, ExecutorError> {
        if bound.group_by.is_some() {
            return Ok(SelectMode::Group(GroupState::new()));
        }

        if bound.order_by.is_some() {
            return Ok(SelectMode::Sort(Vec::new()));
        }

        if bound
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

            return Ok(SelectMode::Aggregate(states));
        }

        Ok(SelectMode::Rows)
    }
}

pub struct SelectState {
    bound: BoundSelect,
    mode: SelectMode,
    emitted: usize,
    done: bool,
}

impl SelectState {
    pub fn new(bound: BoundSelect) -> Result<Self, ExecutorError> {
        let mode = SelectMode::new(&bound)?;

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
        metadata: &DatabaseMetadata,
        row: &QueryRow,
    ) -> Result<SelectStep, ExecutorError> {
        if self.is_done() {
            return Ok(SelectStep::Done);
        }

        if let Some(filter) = &self.bound.filter
            && !row_matches_filter(metadata, row, filter)?
        {
            return Ok(SelectStep::NeedInput);
        }

        match &mut self.mode {
            SelectMode::Rows => {
                let values = project_row(metadata, row, &self.bound.projections)?;
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
                    accumulate_aggregate(metadata, state, projection, row)?;
                }

                Ok(SelectStep::NeedInput)
            }
            SelectMode::Group(group_state) => {
                let Some(group_by) = &self.bound.group_by else {
                    return Err(ExecutorError::Unsupported);
                };

                group_state.push_row(metadata, row, group_by, &self.bound.projections)?;
                Ok(SelectStep::NeedInput)
            }
            SelectMode::Sort(rows) => {
                rows.push(row.clone());
                Ok(SelectStep::NeedInput)
            }
            SelectMode::GroupOutput(_) | SelectMode::SortOutput(_) => {
                Err(ExecutorError::Unsupported)
            }
        }
    }

    pub fn finish(&mut self, metadata: &DatabaseMetadata) -> Result<SelectStep, ExecutorError> {
        if self.is_done() {
            return Ok(SelectStep::Done);
        }

        match &mut self.mode {
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
            SelectMode::Group(group_state) => {
                let Some(group_by) = &self.bound.group_by else {
                    return Err(ExecutorError::Unsupported);
                };

                let group_state = std::mem::replace(group_state, GroupState::new());
                let mut results = group_state.into_rows(
                    group_by,
                    &self.bound.projections,
                    self.bound.order_by.as_ref(),
                )?;

                if let Some(limit) = self.bound.limit {
                    results.truncate(limit);
                }

                self.mode = SelectMode::GroupOutput(results.into_iter());
                self.finish(metadata)
            }
            SelectMode::GroupOutput(values_iter) => {
                if let Some(values) = values_iter.next() {
                    return Ok(SelectStep::Row(values));
                }

                self.done = true;
                Ok(SelectStep::Done)
            }
            SelectMode::Sort(rows) => {
                let collected_rows = std::mem::take(rows);

                let Some(order_by) = self.bound.order_by.as_ref() else {
                    return Err(ExecutorError::Unsupported);
                };

                let mut ordered_rows = order_rows(metadata, collected_rows, order_by)?;
                if let Some(limit) = self.bound.limit {
                    ordered_rows.truncate(limit);
                }

                self.mode = SelectMode::SortOutput(ordered_rows.into_iter());
                self.finish(metadata)
            }
            SelectMode::SortOutput(rows_iter) => {
                if let Some(row) = rows_iter.next() {
                    let values = project_row(metadata, &row, &self.bound.projections)?;
                    return Ok(SelectStep::Row(values));
                }

                self.done = true;
                Ok(SelectStep::Done)
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}
