use crate::query::common::{ComparisonOperator, SortDirection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Select(SelectStatement),
    CreateTable(CreateTableStatement),
    CreateIndex(CreateIndexStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTableStatement {
    pub table: String,
    pub columns: Vec<ColumnDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIndexStatement {
    pub index_name: String,
    pub table: String,
    pub column_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectStatement {
    pub projections: Vec<Projection>,
    pub from: FromClause,
    pub filter: Option<Expression>,
    pub group_by: Option<Vec<ColumnReference>>,
    pub order_by: Option<OrderBy>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertStatement {
    pub table: String,
    pub literals: Vec<Literal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatement {
    pub from: FromClause,
    pub assignments: Vec<Assignment>,
    pub filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteStatement {
    pub targets: Option<Vec<String>>,
    pub from: FromClause,
    pub filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub column: ColumnReference,
    pub value: Literal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projection {
    All,
    Expression(Expression),
    Aggregate(Aggregate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReference {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnReference {
    pub table: Option<String>,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinCondition {
    pub left: ColumnReference,
    pub right: ColumnReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromClause {
    Table(TableReference),
    InnerJoin {
        left: Box<FromClause>,
        on: JoinCondition,
        right: Box<FromClause>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aggregate {
    CountAll,
    Sum(ColumnReference),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Column(ColumnReference),
    Literal(Literal),
    Comparison {
        left: Box<Expression>,
        operator: ComparisonOperator,
        right: Box<Expression>,
    },
    And {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Or {
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    pub column: ColumnReference,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Integer(i64),
    String(String),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: SqlDataType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDataType {
    Int,
    BigInt,
    Boolean,
    Varchar,
    Null,
}
