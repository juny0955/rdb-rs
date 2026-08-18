#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Statement {
    Select(SelectStatement),
    CreateTable(CreateTableStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectStatement {
    pub(crate) projections: Vec<Projection>,
    pub(crate) table: String,
    pub(crate) filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateTableStatement {
    pub(crate) table: String,
    pub(crate) columns: Vec<ColumnDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InsertStatement {
    pub(crate) table: String,
    pub(crate) literals: Vec<Literal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateStatement {
    pub(crate) table: String,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub(crate) column: String,
    pub(crate) value: Literal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Projection {
    All,
    Expression(Expression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expression {
    Identifier(String),
    Literal(Literal),
    Equal {
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Literal {
    Integer(i64),
    String(String),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColumnDefinition {
    pub(crate) name: String,
    pub(crate) data_type: DataType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DataType {
    Int,
    BigInt,
    Boolean,
    Varchar,
    Null,
}
