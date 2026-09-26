use crate::{
    catalog::metadata::{ColumnId, DataType, TableId},
    query::common::{ComparisonOperator, SortDirection},
    tuple::Value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableInstanceId(pub u32);

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
    pub projections: Vec<BoundProjection>,
    pub from: BoundFromClause,
    pub filter: Option<BoundExpression>,
    pub group_by: Option<Vec<BoundColumnReference>>,
    pub order_by: Option<BoundOrderBy>,
    pub limit: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundInsert {
    pub table_id: TableId,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundUpdate {
    pub from: BoundFromClause,
    pub assignments: Vec<BoundAssignment>,
    pub filter: Option<BoundExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundDelete {
    pub targets: Vec<BoundTable>,
    pub from: BoundFromClause,
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
    Column(BoundColumnReference),
    Aggregate(BoundAggregate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundTable {
    pub table_id: TableId,
    pub instance_id: TableInstanceId,
    pub alias: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundColumnReference {
    pub table_id: TableId,
    pub instance_id: TableInstanceId,
    pub column_id: ColumnId,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundJoinCondition {
    pub left: BoundColumnReference,
    pub right: BoundColumnReference,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundFromClause {
    Table(BoundTable),
    InnerJoin {
        left: Box<BoundFromClause>,
        on: BoundJoinCondition,
        right: Box<BoundFromClause>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundAggregate {
    CountAll,
    Sum(BoundColumnReference),
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundExpression {
    Comparison {
        column: BoundColumnReference,
        operator: ComparisonOperator,
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
    pub column: BoundColumnReference,
    pub value: Value,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundColumnDefinition {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoundOrderBy {
    pub column: BoundColumnReference,
    pub direction: SortDirection,
}
