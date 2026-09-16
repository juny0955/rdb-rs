use crate::{
    catalog::metadata::{ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableMetadata},
    query::sql::ast::{
        Aggregate, CreateIndexStatement, DeleteStatement,
        Expression::{self, Identifier},
        InsertStatement, Literal, OrderBy, Projection, SelectStatement, SqlDataType, Statement,
        UpdateStatement,
    },
    tuple::Value,
};
use thiserror::Error;

mod bound;
pub use bound::*;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BinderError {
    #[error("테이블을 찾을 수 없습니다: {0}")]
    TableNotFound(String),
    #[error("테이블 '{table}'에서 컬럼을 찾을 수 없습니다: {column}")]
    ColumnNotFound { table: String, column: String },
    #[error("테이블이 이미 존재합니다: {0}")]
    AlreadyExistsTable(String),
    #[error("값 개수가 컬럼 개수와 일치하지 않습니다 (기대값: {expected}, 실제값: {actual})")]
    ValueCountMismatch { expected: usize, actual: usize },
    #[error("컬럼 '{column}'의 값 타입이 올바르지 않습니다 (기대 타입: {expected:?})")]
    TypeMismatch { column: String, expected: DataType },
    #[error("WHERE 조건식이 올바르지 않습니다")]
    InvalidFilterExpression,
    #[error("SELECT projection이 올바르지 않습니다")]
    InvalidProjectionExpression,
    #[error("SUM은 INT 또는 BIGINT 컬럼에만 사용할 수 있습니다")]
    UnsupportedAggregateType,
    #[error(
        "GROUP BY 또는 aggregate 쿼리의 SELECT 목록에는 GROUP BY 컬럼과 aggregate만 사용할 수 있습니다"
    )]
    InvalidGroupingProjection,
    #[error("GROUP BY 쿼리의 ORDER BY 컬럼은 GROUP BY에 포함되어야 합니다")]
    InvalidGroupingOrder,
    #[error("GROUP BY 없는 aggregate 쿼리에서는 원본 컬럼으로 정렬할 수 없습니다")]
    InvalidAggregateOrder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binder<'a> {
    database: &'a DatabaseMetadata,
}

impl<'a> Binder<'a> {
    pub fn new(database: &'a DatabaseMetadata) -> Self {
        Self { database }
    }

    pub fn bind(&self, statement: &Statement) -> Result<BoundStatement, BinderError> {
        match statement {
            Statement::CreateTable(s) => {
                if self.database.table(statement.table()).is_some() {
                    return Err(BinderError::AlreadyExistsTable(
                        statement.table().to_owned(),
                    ));
                }

                Ok(BoundStatement::CreateTable(BoundCreateTable {
                    table: s.table.clone(),
                    columns: s
                        .columns
                        .iter()
                        .map(|column| BoundColumnDefinition {
                            name: column.name.clone(),
                            data_type: bind_data_type(column.data_type),
                        })
                        .collect(),
                }))
            }
            Statement::CreateIndex(s) => {
                Ok(BoundStatement::CreateIndex(self.bind_create_index(s)?))
            }
            Statement::Select(s) => Ok(BoundStatement::Select(self.bind_select(s)?)),
            Statement::Insert(s) => Ok(BoundStatement::Insert(self.bind_insert(s)?)),
            Statement::Update(s) => Ok(BoundStatement::Update(self.bind_update(s)?)),
            Statement::Delete(s) => Ok(BoundStatement::Delete(self.bind_delete(s)?)),
        }
    }

    fn bind_select(&self, statement: &SelectStatement) -> Result<BoundSelect, BinderError> {
        let table = self.require_table(&statement.table)?;
        let table_id = table.id();

        let projections = bind_projections(table, statement)?;

        let filter = if let Some(filter) = &statement.filter {
            Some(bind_filter(table, filter)?)
        } else {
            None
        };

        let group_by = bind_group_by(table, &statement.group_by)?;
        validate_projection(&projections, &group_by)?;

        let order_by = bind_order_by(table, &statement.order_by)?;
        validate_order_by(&projections, &group_by, &order_by)?;

        Ok(BoundSelect {
            table_id,
            projections,
            filter,
            group_by,
            order_by,
            limit: statement.limit,
        })
    }

    fn bind_insert(&self, statement: &InsertStatement) -> Result<BoundInsert, BinderError> {
        let table = self.require_table(&statement.table)?;
        let table_id = table.id();

        let literals = &statement.literals;
        if literals.len() != table.columns().len() {
            let expected = table.columns().len();
            let actual = literals.len();
            return Err(BinderError::ValueCountMismatch { expected, actual });
        }

        let mut values = Vec::new();
        for (literal, column) in literals.iter().zip(table.columns()) {
            values.push(bind_value(literal, column)?);
        }

        Ok(BoundInsert { table_id, values })
    }

    fn bind_delete(&self, statement: &DeleteStatement) -> Result<BoundDelete, BinderError> {
        let table = self.require_table(&statement.table)?;
        let table_id = table.id();

        let filter = if let Some(filter) = &statement.filter {
            Some(bind_filter(table, filter)?)
        } else {
            None
        };

        Ok(BoundDelete { table_id, filter })
    }

    fn bind_update(&self, statement: &UpdateStatement) -> Result<BoundUpdate, BinderError> {
        let table = self.require_table(&statement.table)?;
        let table_id = table.id();

        let mut assignments = Vec::new();
        for assignment in &statement.assignments {
            match table.column(&assignment.column) {
                Some(column) => {
                    let value = bind_value(&assignment.value, column)?;
                    assignments.push(BoundAssignment {
                        column_id: column.id(),
                        value,
                    });
                }
                None => {
                    return Err(BinderError::ColumnNotFound {
                        table: table.name().to_owned(),
                        column: assignment.column.to_owned(),
                    });
                }
            }
        }

        let filter = if let Some(filter) = &statement.filter {
            Some(bind_filter(table, filter)?)
        } else {
            None
        };

        Ok(BoundUpdate {
            table_id,
            assignments,
            filter,
        })
    }

    fn bind_create_index(
        &self,
        statement: &CreateIndexStatement,
    ) -> Result<BoundCreateIndex, BinderError> {
        let table = self.require_table(&statement.table)?;
        let column = table
            .column(&statement.column_name)
            .ok_or(BinderError::ColumnNotFound {
                table: statement.table.to_owned(),
                column: statement.column_name.to_owned(),
            })?;

        Ok(BoundCreateIndex {
            index_name: statement.index_name.to_owned(),
            table_id: table.id(),
            column_id: column.id(),
        })
    }

    fn require_table(&self, name: &str) -> Result<&TableMetadata, BinderError> {
        let table = self.database.table(name);
        if table.is_none() {
            return Err(BinderError::TableNotFound(name.to_string()));
        }

        Ok(table.unwrap())
    }
}

fn validate_projection(
    projections: &[BoundProjection],
    group_by: &Option<Vec<ColumnId>>,
) -> Result<(), BinderError> {
    let has_aggregate = projections
        .iter()
        .any(|projection| matches!(projection, BoundProjection::Aggregate(_)));

    for projection in projections {
        match projection {
            BoundProjection::Column(id) => {
                if (has_aggregate || group_by.is_some())
                    && !group_by
                        .as_ref()
                        .is_some_and(|columns| columns.contains(id))
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
    group_by: &Option<Vec<ColumnId>>,
    order_by: &Option<BoundOrderBy>,
) -> Result<(), BinderError> {
    let Some(order_by) = order_by else {
        return Ok(());
    };

    if let Some(group_by) = group_by {
        if group_by.contains(&order_by.column_id) {
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

fn bind_order_by(
    table: &TableMetadata,
    order: &Option<OrderBy>,
) -> Result<Option<BoundOrderBy>, BinderError> {
    if let Some(order) = &order {
        let Some(column) = table.column(&order.column) else {
            return Err(BinderError::ColumnNotFound {
                table: table.name().to_owned(),
                column: order.column.to_owned(),
            });
        };

        Ok(Some(BoundOrderBy {
            column_id: column.id(),
            direction: order.direction,
        }))
    } else {
        Ok(None)
    }
}

fn bind_projections(
    table: &TableMetadata,
    statement: &SelectStatement,
) -> Result<Vec<BoundProjection>, BinderError> {
    let mut projections = Vec::new();

    for projection in &statement.projections {
        match projection {
            Projection::All => projections.push(BoundProjection::All),
            Projection::Expression(Identifier(column_name)) => match table.column(column_name) {
                Some(column) => projections.push(BoundProjection::Column(column.id())),
                None => {
                    return Err(BinderError::ColumnNotFound {
                        table: statement.table.to_owned(),
                        column: column_name.to_owned(),
                    });
                }
            },
            Projection::Aggregate(aggreate) => match aggreate {
                Aggregate::CountAll => {
                    projections.push(BoundProjection::Aggregate(BoundAggregate::CountAll))
                }
                Aggregate::Sum(column_name) => match table.column(column_name) {
                    Some(column) => match column.data_type() {
                        DataType::Int | DataType::BigInt => projections
                            .push(BoundProjection::Aggregate(BoundAggregate::Sum(column.id()))),
                        _ => return Err(BinderError::UnsupportedAggregateType),
                    },
                    None => {
                        return Err(BinderError::ColumnNotFound {
                            table: statement.table.to_owned(),
                            column: column_name.to_owned(),
                        });
                    }
                },
            },
            Projection::Expression(_) => return Err(BinderError::InvalidProjectionExpression),
        }
    }

    Ok(projections)
}

fn bind_group_by(
    table: &TableMetadata,
    columns: &Option<Vec<String>>,
) -> Result<Option<Vec<ColumnId>>, BinderError> {
    if let Some(columns) = columns {
        let mut column_ids = Vec::new();
        for column_name in columns {
            let column = table
                .column(column_name)
                .ok_or(BinderError::ColumnNotFound {
                    table: table.name().to_owned(),
                    column: column_name.to_owned(),
                })?;

            column_ids.push(column.id())
        }

        return Ok(Some(column_ids));
    }

    Ok(None)
}

fn bind_filter(table: &TableMetadata, filter: &Expression) -> Result<BoundExpression, BinderError> {
    match filter {
        Expression::And { left, right } => Ok(BoundExpression::And {
            left: Box::new(bind_filter(table, left)?),
            right: Box::new(bind_filter(table, right)?),
        }),
        Expression::Or { left, right } => Ok(BoundExpression::Or {
            left: Box::new(bind_filter(table, left)?),
            right: Box::new(bind_filter(table, right)?),
        }),
        Expression::Comparison {
            left,
            operator,
            right,
        } => {
            let (Expression::Identifier(column_name), Expression::Literal(literal)) =
                (left.as_ref(), right.as_ref())
            else {
                return Err(BinderError::InvalidFilterExpression);
            };

            let Some(column) = table.column(column_name) else {
                return Err(BinderError::ColumnNotFound {
                    table: table.name().to_owned(),
                    column: column_name.to_owned(),
                });
            };

            let value = bind_value(literal, column)?;
            Ok(BoundExpression::Comparison {
                column_id: column.id(),
                operator: *operator,
                value,
            })
        }
        _ => Err(BinderError::InvalidFilterExpression),
    }
}

fn bind_value(literal: &Literal, column: &ColumnMetadata) -> Result<Value, BinderError> {
    Ok(match (literal, column.data_type()) {
        (Literal::Null, _) => Value::Null,
        (Literal::Integer(v), DataType::Int) => {
            let v = i32::try_from(*v).map_err(|_| BinderError::TypeMismatch {
                column: column.name().to_owned(),
                expected: column.data_type(),
            })?;

            Value::Int(v)
        }
        (Literal::Integer(v), DataType::BigInt) => Value::BigInt(*v),
        (Literal::String(v), DataType::Varchar) => Value::Varchar(v.to_owned()),
        _ => {
            return Err(BinderError::TypeMismatch {
                column: column.name().to_owned(),
                expected: column.data_type(),
            });
        }
    })
}

fn bind_data_type(sql_data_type: SqlDataType) -> DataType {
    match sql_data_type {
        SqlDataType::Int => DataType::Int,
        SqlDataType::BigInt => DataType::BigInt,
        SqlDataType::Boolean => DataType::Boolean,
        SqlDataType::Varchar => DataType::Varchar,
        SqlDataType::Null => DataType::Null,
    }
}

#[cfg(test)]
mod tests;
