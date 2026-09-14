use crate::{
    catalog::metadata::{ColumnId, DataType, TableId},
    tuple::Value,
};

#[derive(Debug, PartialEq, Eq)]
pub enum BoundStatement {
    CreateTable(BoundCreateTable),
    CreateIndex(BoundCreateIndex),
    Insert(BoundInsert),
    Select(BoundSelect),
    Update(BoundUpdate),
    Delete(BoundDelete),
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundSelect {
    pub table_id: TableId,
    pub projections: Vec<BoundProjection>,
    pub filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundInsert {
    pub table_id: TableId,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundUpdate {
    pub table_id: TableId,
    pub assignments: Vec<BoundAssignment>,
    pub filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundDelete {
    pub table_id: TableId,
    pub filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundCreateTable {
    pub table: String,
    pub columns: Vec<BoundColumnDefinition>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundCreateIndex {
    pub index_name: String,
    pub table_id: TableId,
    pub column_id: ColumnId,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundProjection {
    All,
    Column(ColumnId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundOperator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundExpression {
    Comparison {
        column_id: ColumnId,
        operator: BoundOperator,
        value: Value,
    },
    And {
        left: Box<BoundExpression>,
        right: Box<BoundExpression>,
    },
    Or {
        left: Box<BoundExpression>,
        right: Box<BoundExpression>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundAssignment {
    pub column_id: ColumnId,
    pub value: Value,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundColumnDefinition {
    pub name: String,
    pub data_type: DataType,
}
