use crate::{
    parser::ast::{ColumnDefinition, Literal},
    schema::{ColumnId, TableId},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BoundStatement {
    CreateTable(BoundCreateTable),
    Insert(BoundInsert),
    Select(BoundSelect),
    Update(BoundUpdate),
    Delete(BoundDelete),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundSelect {
    pub(crate) table_id: TableId,
    pub(crate) projections: Vec<BoundProjection>,
    pub(crate) filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundInsert {
    pub(crate) table_id: TableId,
    pub(crate) literals: Vec<Literal>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundUpdate {
    pub(crate) table_id: TableId,
    pub(crate) assignments: Vec<BoundAssignment>,
    pub(crate) filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundDelete {
    pub(crate) table_id: TableId,
    pub(crate) filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundCreateTable {
    pub(crate) table: String,
    pub(crate) columns: Vec<ColumnDefinition>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BoundProjection {
    All,
    Column(ColumnId),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BoundExpression {
    Equal { column_id: ColumnId, value: Literal },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundAssignment {
    pub(crate) column_id: ColumnId,
    pub(crate) value: Literal,
}
