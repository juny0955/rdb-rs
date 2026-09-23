use crate::{
    catalog::metadata::DataType,
    query::{
        binder::{
            Binder, BinderError, BoundAggregate, BoundColumnReference, BoundExpression,
            BoundOrderBy, BoundProjection, BoundSelect, BoundTable, bind_value, collect_tables,
        },
        sql::ast::{Aggregate, ColumnReference, Expression, OrderBy, Projection, SelectStatement},
    },
};

impl<'a> Binder<'a> {
    pub(super) fn bind_select(
        &self,
        statement: &SelectStatement,
    ) -> Result<BoundSelect, BinderError> {
        let mut next_instance_id = 0;
        let from = self.bind_from_clause(&statement.from, &mut next_instance_id)?;
        let mut tables = Vec::new();
        collect_tables(&from, &mut tables);

        let projections = self.bind_projections(&tables, statement)?;

        let filter = statement
            .filter
            .as_ref()
            .map(|filter| self.bind_filter(&tables, filter))
            .transpose()?;

        let group_by = self.bind_group_by(&tables, &statement.group_by)?;
        validate_projection(&projections, &group_by)?;

        let order_by = self.bind_order_by(&tables, &statement.order_by)?;
        validate_order_by(&projections, &group_by, &order_by)?;

        Ok(BoundSelect {
            projections,
            from,
            filter,
            group_by,
            order_by,
            limit: statement.limit,
        })
    }

    pub(super) fn bind_filter(
        &self,
        tables: &[&BoundTable],
        filter: &Expression,
    ) -> Result<BoundExpression, BinderError> {
        match filter {
            Expression::And { left, right } => Ok(BoundExpression::And {
                left: Box::new(self.bind_filter(tables, left)?),
                right: Box::new(self.bind_filter(tables, right)?),
            }),
            Expression::Or { left, right } => Ok(BoundExpression::Or {
                left: Box::new(self.bind_filter(tables, left)?),
                right: Box::new(self.bind_filter(tables, right)?),
            }),
            Expression::Comparison {
                left,
                operator,
                right,
            } => {
                let (Expression::Column(column_ref), Expression::Literal(literal)) =
                    (left.as_ref(), right.as_ref())
                else {
                    return Err(BinderError::InvalidFilterExpression);
                };

                let column = self.bind_column_reference(tables, column_ref)?;
                let column_meta = self.require_column(&column)?;
                let value = bind_value(literal, column_meta)?;

                Ok(BoundExpression::Comparison {
                    column,
                    operator: *operator,
                    value,
                })
            }
            _ => Err(BinderError::InvalidFilterExpression),
        }
    }

    fn bind_projections(
        &self,
        tables: &[&BoundTable],
        statement: &SelectStatement,
    ) -> Result<Vec<BoundProjection>, BinderError> {
        let mut projections = Vec::new();

        for projection in &statement.projections {
            match projection {
                Projection::All => projections.push(BoundProjection::All),
                Projection::Expression(Expression::Column(column_ref)) => {
                    let column = self.bind_column_reference(tables, column_ref)?;

                    projections.push(BoundProjection::Column(column));
                }
                Projection::Aggregate(aggreate) => match aggreate {
                    Aggregate::CountAll => {
                        projections.push(BoundProjection::Aggregate(BoundAggregate::CountAll))
                    }
                    Aggregate::Sum(column_ref) => {
                        let column = self.bind_column_reference(tables, column_ref)?;
                        let column_meta = self.require_column(&column)?;
                        match column_meta.data_type() {
                            DataType::Int | DataType::BigInt => projections
                                .push(BoundProjection::Aggregate(BoundAggregate::Sum(column))),
                            _ => return Err(BinderError::UnsupportedAggregateType),
                        }
                    }
                },
                Projection::Expression(_) => return Err(BinderError::InvalidProjectionExpression),
            }
        }

        Ok(projections)
    }

    fn bind_group_by(
        &self,
        tables: &[&BoundTable],
        columns: &Option<Vec<ColumnReference>>,
    ) -> Result<Option<Vec<BoundColumnReference>>, BinderError> {
        if let Some(column_refs) = columns {
            let mut columns = Vec::new();
            for column_ref in column_refs {
                let column = self.bind_column_reference(tables, column_ref)?;
                columns.push(column);
            }

            return Ok(Some(columns));
        }

        Ok(None)
    }

    fn bind_order_by(
        &self,
        tables: &[&BoundTable],
        order: &Option<OrderBy>,
    ) -> Result<Option<BoundOrderBy>, BinderError> {
        if let Some(order) = &order {
            let column = self.bind_column_reference(tables, &order.column)?;

            return Ok(Some(BoundOrderBy {
                column,
                direction: order.direction,
            }));
        }

        Ok(None)
    }
}

fn validate_projection(
    projections: &[BoundProjection],
    group_by: &Option<Vec<BoundColumnReference>>,
) -> Result<(), BinderError> {
    let has_aggregate = projections
        .iter()
        .any(|projection| matches!(projection, BoundProjection::Aggregate(_)));

    for projection in projections {
        match projection {
            BoundProjection::Column(column_ref) => {
                if (has_aggregate || group_by.is_some())
                    && !group_by
                        .as_ref()
                        .is_some_and(|columns| columns.contains(column_ref))
                {
                    return Err(BinderError::InvalidGroupingProjection);
                }
            }
            BoundProjection::All => {
                if has_aggregate || group_by.is_some() {
                    return Err(BinderError::InvalidGroupingProjection);
                }
            }
            _ => continue,
        }
    }

    Ok(())
}

fn validate_order_by(
    projections: &[BoundProjection],
    group_by: &Option<Vec<BoundColumnReference>>,
    order_by: &Option<BoundOrderBy>,
) -> Result<(), BinderError> {
    let Some(order_by) = order_by else {
        return Ok(());
    };

    if let Some(group_by) = group_by {
        if group_by.contains(&order_by.column) {
            return Ok(());
        }

        return Err(BinderError::InvalidGroupingOrder);
    }

    if projections
        .iter()
        .any(|projection| matches!(projection, BoundProjection::Aggregate(_)))
    {
        return Err(BinderError::InvalidAggregateOrder);
    }

    Ok(())
}
