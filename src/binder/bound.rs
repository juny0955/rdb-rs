use crate::{
    catalog::metadata::{ColumnId, TableId},
    sql::ast::{ColumnDefinition, Literal},
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
    pub literals: Vec<Literal>,
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
    pub columns: Vec<ColumnDefinition>,
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
pub enum BoundExpression {
    Equal { column_id: ColumnId, value: Literal },
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundAssignment {
    pub column_id: ColumnId,
    pub value: Literal,
}
