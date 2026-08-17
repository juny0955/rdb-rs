#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Literal {
    Integer(i64),
    String(String),
    Null,
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
pub(crate) enum Projection {
    All,
    Expression(Expression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectStatement {
    pub(crate) projections: Vec<Projection>,
    pub(crate) table: String,
    pub(crate) filter: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Statement {
    Select(SelectStatement),
}
