#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Select(SelectStatement),
    CreateTable(CreateTableStatement),
    CreateIndex(CreateIndexStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
}

impl Statement {
    pub fn table(&self) -> &str {
        match self {
            Self::Select(s) => &s.table,
            Self::CreateTable(s) => &s.table,
            Self::CreateIndex(s) => &s.table,
            Self::Insert(s) => &s.table,
            Self::Update(s) => &s.table,
            Self::Delete(s) => &s.table,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectStatement {
    pub projections: Vec<Projection>,
    pub table: String,
    pub filter: Option<Expression>,
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
pub struct InsertStatement {
    pub table: String,
    pub literals: Vec<Literal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatement {
    pub table: String,
    pub assignments: Vec<Assignment>,
    pub filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteStatement {
    pub table: String,
    pub filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub column: String,
    pub value: Literal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projection {
    All,
    Expression(Expression),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Identifier(String),
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
