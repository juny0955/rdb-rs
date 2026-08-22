use crate::{
    parser::ast::{
        DeleteStatement,
        Expression::{self, Identifier},
        InsertStatement, Literal, Projection, SelectStatement, Statement, UpdateStatement,
    },
    schema::{DataType, DatabaseMetadata, TableMetadata},
};

mod bound;
pub(crate) use bound::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinderError {
    TableNotFound(String),
    ColumnNotFound { table: String, column: String },
    AlreadyExistsTable(String),
    ValueCountMismatch { expected: usize, actual: usize },
    TypeMismatch { column: String, expected: DataType },
    InvalidFilterExpression,
    InvalidProjectionExpression,
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
                    columns: s.columns.clone(),
                }))
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
        let mut projections = Vec::new();

        for projection in &statement.projections {
            match projection {
                Projection::All => projections.push(BoundProjection::All),
                Projection::Expression(Identifier(column)) => match table.column(column) {
                    Some(column) => projections.push(BoundProjection::Column(column.id())),
                    None => {
                        return Err(BinderError::ColumnNotFound {
                            table: statement.table.to_owned(),
                            column: column.to_owned(),
                        });
                    }
                },
                Projection::Expression(_) => return Err(BinderError::InvalidProjectionExpression),
            }
        }

        let filter = if let Some(filter) = &statement.filter {
            Some(Self::bind_filter(table, filter)?)
        } else {
            None
        };

        Ok(BoundSelect {
            table_id,
            projections,
            filter,
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

        for (literal, column) in literals.iter().zip(table.columns()) {
            if !Self::is_compatible(literal, column.data_type()) {
                return Err(BinderError::TypeMismatch {
                    column: column.name().to_owned(),
                    expected: column.data_type(),
                });
            }
        }

        Ok(BoundInsert {
            table_id,
            literals: literals.clone(),
        })
    }

    fn bind_delete(&self, statement: &DeleteStatement) -> Result<BoundDelete, BinderError> {
        let table = self.require_table(&statement.table)?;
        let table_id = table.id();

        let filter = if let Some(filter) = &statement.filter {
            Some(Self::bind_filter(table, filter)?)
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
                    if !Self::is_compatible(&assignment.value, column.data_type()) {
                        return Err(BinderError::TypeMismatch {
                            column: column.name().to_owned(),
                            expected: column.data_type(),
                        });
                    }
                    assignments.push(BoundAssignment {
                        column_id: column.id(),
                        value: assignment.value.clone(),
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
            Some(Self::bind_filter(table, filter)?)
        } else {
            None
        };

        Ok(BoundUpdate {
            table_id,
            assignments,
            filter,
        })
    }

    fn bind_filter(
        table: &TableMetadata,
        filter: &Expression,
    ) -> Result<BoundExpression, BinderError> {
        let Expression::Equal { left, right } = filter else {
            return Err(BinderError::InvalidFilterExpression);
        };

        let Expression::Identifier(column_name) = left.as_ref() else {
            return Err(BinderError::InvalidFilterExpression);
        };
        let Expression::Literal(literal) = right.as_ref() else {
            return Err(BinderError::InvalidFilterExpression);
        };

        let Some(column) = table.column(column_name) else {
            return Err(BinderError::ColumnNotFound {
                table: table.name().to_owned(),
                column: column_name.to_owned(),
            });
        };

        if !Self::is_compatible(literal, column.data_type()) {
            return Err(BinderError::TypeMismatch {
                column: column_name.to_owned(),
                expected: column.data_type(),
            });
        }
        Ok(BoundExpression::Equal {
            column_id: column.id(),
            value: literal.clone(),
        })
    }

    fn is_compatible(literal: &Literal, data_type: DataType) -> bool {
        match (literal, data_type) {
            (Literal::Null, _) => true,
            (Literal::Integer(value), DataType::Int) => i32::try_from(*value).is_ok(),
            (Literal::Integer(_), DataType::BigInt) => true,
            (Literal::String(_), DataType::Varchar) => true,
            _ => false,
        }
    }

    fn require_table(&self, name: &str) -> Result<&TableMetadata, BinderError> {
        let table = self.database.table(name);
        if table.is_none() {
            return Err(BinderError::TableNotFound(name.to_string()));
        }

        Ok(table.unwrap())
    }
}

#[cfg(test)]
mod tests;
