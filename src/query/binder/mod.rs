use std::collections::HashSet;

use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata,
    },
    query::sql::ast::{
        ColumnReference, CreateIndexStatement, CreateTableStatement, DeleteStatement, FromClause,
        InsertStatement, JoinCondition, Literal, SqlDataType, Statement, UpdateStatement,
    },
    tuple::Value,
};
use thiserror::Error;

mod bound;
mod select;
pub use bound::*;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BinderError {
    #[error("테이블을 찾을 수 없습니다: {0}")]
    TableNotFound(String),
    #[error("테이블 ID를 찾을 수 없습니다: {0:?}")]
    TableIdNotFound(TableId),
    #[error("테이블 '{table}'에서 컬럼을 찾을 수 없습니다: {column}")]
    ColumnNotFound { table: String, column: String },
    #[error("테이블 ID {table_id:?}에서 컬럼 ID를 찾을 수 없습니다: {column_id:?}")]
    ColumnIdNotFound {
        table_id: TableId,
        column_id: ColumnId,
    },
    #[error("FROM 절에서 컬럼을 찾을 수 없습니다: {column}")]
    ColumnNotFoundInScope { column: String },
    #[error("컬럼 참조가 모호합니다: {column}")]
    AmbiguousColumn { column: String },
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
    #[error("JOIN절 양쪽 data type이 다릅니다: left: {left:?}, right: {right:?}")]
    JoinColumnTypeMismatch { left: DataType, right: DataType },
    #[error("FROM 절 테이블 참조 이름이 중복되었습니다: {0}")]
    DuplicateTableReference(String),
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
        Ok(match statement {
            Statement::CreateTable(s) => BoundStatement::CreateTable(self.bind_create_table(s)?),
            Statement::CreateIndex(s) => BoundStatement::CreateIndex(self.bind_create_index(s)?),
            Statement::Select(s) => BoundStatement::Select(self.bind_select(s)?),
            Statement::Insert(s) => BoundStatement::Insert(self.bind_insert(s)?),
            Statement::Update(s) => BoundStatement::Update(self.bind_update(s)?),
            Statement::Delete(s) => BoundStatement::Delete(self.bind_delete(s)?),
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
        let mut next_instance_id = 0;
        let from = self.bind_from_clause(&statement.from, &mut next_instance_id)?;
        let mut tables = Vec::new();
        collect_tables(&from, &mut tables);

        let targets = match statement.targets.as_ref() {
            Some(target_names) => {
                let mut targets = Vec::new();

                for target_name in target_names {
                    let mut found = None;

                    for table in &tables {
                        let qualifier = self.table_reference_name(table)?;
                        if qualifier == target_name {
                            found = Some(*table);
                            break;
                        }
                    }

                    let table =
                        found.ok_or_else(|| BinderError::TableNotFound(target_name.to_owned()))?;
                    targets.push((*table).clone());
                }
                targets
            }
            None => {
                vec![leftmost_table(&from).clone()]
            }
        };

        let filter = statement
            .filter
            .as_ref()
            .map(|filter| self.bind_filter(&tables, filter))
            .transpose()?;

        Ok(BoundDelete {
            targets,
            from,
            filter,
        })
    }

    fn bind_update(&self, statement: &UpdateStatement) -> Result<BoundUpdate, BinderError> {
        let mut next_instance_id = 0;
        let from = self.bind_from_clause(&statement.from, &mut next_instance_id)?;
        let mut tables = Vec::new();
        collect_tables(&from, &mut tables);

        let mut assignments = Vec::new();
        for assignment in &statement.assignments {
            let column = self.bind_column_reference(&tables, &assignment.column)?;
            let column_meta = self.require_column(&column)?;
            let value = bind_value(&assignment.value, column_meta)?;
            assignments.push(BoundAssignment { column, value });
        }

        let filter = statement
            .filter
            .as_ref()
            .map(|filter| self.bind_filter(&tables, filter))
            .transpose()?;

        Ok(BoundUpdate {
            from,
            assignments,
            filter,
        })
    }

    fn bind_create_table(
        &self,
        statement: &CreateTableStatement,
    ) -> Result<BoundCreateTable, BinderError> {
        if self.database.table(&statement.table).is_some() {
            return Err(BinderError::AlreadyExistsTable(statement.table.to_owned()));
        }

        let columns = statement
            .columns
            .iter()
            .map(|column| BoundColumnDefinition {
                name: column.name.clone(),
                data_type: bind_data_type(column.data_type),
            })
            .collect();

        Ok(BoundCreateTable {
            table: statement.table.clone(),
            columns,
        })
    }

    fn bind_create_index(
        &self,
        statement: &CreateIndexStatement,
    ) -> Result<BoundCreateIndex, BinderError> {
        let table = self.require_table(&statement.table)?;
        let column = require_column_by_name(table, &statement.column_name)?;

        Ok(BoundCreateIndex {
            index_name: statement.index_name.to_owned(),
            table_id: table.id(),
            column_id: column.id(),
        })
    }

    fn bind_from_clause(
        &self,
        from_clause: &FromClause,
        next_instance_id: &mut u32,
    ) -> Result<BoundFromClause, BinderError> {
        Ok(match from_clause {
            FromClause::Table(table_ref) => {
                let table_id = self.require_table(&table_ref.name)?.id();
                let instance_id = TableInstanceId(*next_instance_id);
                *next_instance_id += 1;
                BoundFromClause::Table(BoundTable {
                    table_id,
                    alias: table_ref.alias.clone(),
                    instance_id,
                })
            }
            FromClause::InnerJoin { left, on, right } => {
                let bound_left = self.bind_from_clause(left, next_instance_id)?;
                let bound_right = self.bind_from_clause(right, next_instance_id)?;
                let bound_on = self.bind_join_condition(&bound_left, &bound_right, on)?;

                BoundFromClause::InnerJoin {
                    left: Box::new(bound_left),
                    right: Box::new(bound_right),
                    on: bound_on,
                }
            }
        })
    }

    fn bind_join_condition(
        &self,
        left: &BoundFromClause,
        right: &BoundFromClause,
        condition: &JoinCondition,
    ) -> Result<BoundJoinCondition, BinderError> {
        let mut tables = Vec::new();
        collect_tables(left, &mut tables);
        collect_tables(right, &mut tables);

        let mut table_set = HashSet::new();
        for table in &tables {
            let qualifier = self.table_reference_name(table)?;

            if !table_set.insert(qualifier) {
                return Err(BinderError::DuplicateTableReference(qualifier.to_owned()));
            }
        }

        let left_column = self.bind_column_reference(&tables, &condition.left)?;
        let right_column = self.bind_column_reference(&tables, &condition.right)?;

        let left_type = self.require_column(&left_column)?.data_type();
        let right_type = self.require_column(&right_column)?.data_type();
        if left_type != right_type {
            return Err(BinderError::JoinColumnTypeMismatch {
                left: left_type,
                right: right_type,
            });
        }

        Ok(BoundJoinCondition {
            left: left_column,
            right: right_column,
        })
    }

    fn bind_column_reference(
        &self,
        tables: &[&BoundTable],
        column_ref: &ColumnReference,
    ) -> Result<BoundColumnReference, BinderError> {
        match column_ref.table.as_deref() {
            Some(alias) => self.bind_qualified_column_reference(tables, column_ref, alias),
            None => self.bind_unqualified_column_reference(tables, column_ref),
        }
    }

    fn bind_qualified_column_reference(
        &self,
        tables: &[&BoundTable],
        column_ref: &ColumnReference,
        alias: &str,
    ) -> Result<BoundColumnReference, BinderError> {
        let mut bound_table = None;
        for table in tables {
            if self.table_reference_name(table)? == alias {
                bound_table = Some(table);
                break;
            }
        }

        let bound_table =
            bound_table.ok_or_else(|| BinderError::TableNotFound(alias.to_owned()))?;

        let table_meta = self.require_table_by_id(bound_table.table_id)?;
        let column_meta =
            table_meta
                .column(&column_ref.column)
                .ok_or(BinderError::ColumnNotFound {
                    table: alias.to_owned(),
                    column: column_ref.column.to_owned(),
                })?;

        Ok(BoundColumnReference {
            table_id: table_meta.id(),
            instance_id: bound_table.instance_id,
            column_id: column_meta.id(),
        })
    }

    fn bind_unqualified_column_reference(
        &self,
        tables: &[&BoundTable],
        column_ref: &ColumnReference,
    ) -> Result<BoundColumnReference, BinderError> {
        let mut columns = Vec::new();
        for bound_table in tables {
            let table_meta = self.require_table_by_id(bound_table.table_id)?;

            if let Some(column_meta) = table_meta.column(&column_ref.column) {
                columns.push(BoundColumnReference {
                    table_id: table_meta.id(),
                    instance_id: bound_table.instance_id,
                    column_id: column_meta.id(),
                });
            }
        }

        match columns.len() {
            0 => Err(BinderError::ColumnNotFoundInScope {
                column: column_ref.column.to_owned(),
            }),
            1 => columns.pop().ok_or(BinderError::ColumnNotFoundInScope {
                column: column_ref.column.to_owned(),
            }),
            _ => Err(BinderError::AmbiguousColumn {
                column: column_ref.column.to_owned(),
            }),
        }
    }

    fn table_reference_name<'b>(&'b self, table: &'b BoundTable) -> Result<&'b str, BinderError> {
        Ok(match table.alias.as_deref() {
            Some(table_alias) => table_alias,
            None => self.require_table_by_id(table.table_id)?.name(),
        })
    }

    fn require_table(&self, name: &str) -> Result<&TableMetadata, BinderError> {
        self.database
            .table(name)
            .ok_or(BinderError::TableNotFound(name.to_string()))
    }

    fn require_table_by_id(&self, table_id: TableId) -> Result<&TableMetadata, BinderError> {
        self.database
            .table_by_id(table_id)
            .ok_or(BinderError::TableIdNotFound(table_id))
    }

    fn require_column(
        &self,
        column: &BoundColumnReference,
    ) -> Result<&ColumnMetadata, BinderError> {
        let table_meta = self.require_table_by_id(column.table_id)?;
        table_meta
            .column_by_id(column.column_id)
            .ok_or(BinderError::ColumnIdNotFound {
                table_id: table_meta.id(),
                column_id: column.column_id,
            })
    }
}

fn leftmost_table(from: &BoundFromClause) -> &BoundTable {
    match from {
        BoundFromClause::Table(table) => table,
        BoundFromClause::InnerJoin { left, .. } => leftmost_table(left),
    }
}

fn collect_tables<'a>(from: &'a BoundFromClause, tables: &mut Vec<&'a BoundTable>) {
    match from {
        BoundFromClause::Table(table) => tables.push(table),
        BoundFromClause::InnerJoin { left, right, .. } => {
            collect_tables(left, tables);
            collect_tables(right, tables);
        }
    }
}

fn require_column_by_name<'a>(
    table: &'a TableMetadata,
    name: &'a str,
) -> Result<&'a ColumnMetadata, BinderError> {
    table.column(name).ok_or(BinderError::ColumnNotFound {
        table: table.name().to_owned(),
        column: name.to_owned(),
    })
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
