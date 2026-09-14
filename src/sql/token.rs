#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub offset: usize,
}

impl Token {
    pub fn new(kind: TokenKind, offset: usize) -> Self {
        Self { kind, offset }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // keyword
    Create,
    Table,
    Insert,
    Into,
    Index,
    On,
    Values,
    Select,
    From,
    Where,
    Update,
    Set,
    Delete,
    Int,
    BigInt,
    Boolean,
    Varchar,
    Null,

    // value
    Identifier(String),
    Integer(i64),
    StringLiteral(String),

    // symbol
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Asterisk,
    And,
    Or,

    // operator
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,

    Eof,
}

impl From<&str> for TokenKind {
    fn from(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "CREATE" => Self::Create,
            "TABLE" => Self::Table,
            "INSERT" => Self::Insert,
            "INTO" => Self::Into,
            "INDEX" => Self::Index,
            "ON" => Self::On,
            "VALUES" => Self::Values,
            "SELECT" => Self::Select,
            "FROM" => Self::From,
            "WHERE" => Self::Where,
            "UPDATE" => Self::Update,
            "SET" => Self::Set,
            "DELETE" => Self::Delete,
            "INT" => Self::Int,
            "BIGINT" => Self::BigInt,
            "BOOLEAN" => Self::Boolean,
            "VARCHAR" => Self::Varchar,
            "AND" => Self::And,
            "OR" => Self::Or,
            "NULL" => Self::Null,
            _ => Self::Identifier(value.to_owned()),
        }
    }
}
