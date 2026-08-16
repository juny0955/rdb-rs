#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) offset: usize,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, offset: usize) -> Self {
        Self { kind, offset }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    // keyword
    Create,
    Table,
    Insert,
    Into,
    Values,
    Select,
    From,
    Where,
    Update,
    Set,
    Delete,
    Null,

    // value
    Identifier(String),
    Integer(i64),
    StringLiteral(String),

    // symbol / operator
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Asterisk,
    Eq,

    Eof,
}

impl From<&str> for TokenKind {
    fn from(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "CREATE" => Self::Create,
            "TABLE" => Self::Table,
            "INSERT" => Self::Insert,
            "INTO" => Self::Into,
            "VALUES" => Self::Values,
            "SELECT" => Self::Select,
            "FROM" => Self::From,
            "WHERE" => Self::Where,
            "UPDATE" => Self::Update,
            "SET" => Self::Set,
            "DELETE" => Self::Delete,
            "NULL" => Self::Null,
            _ => Self::Identifier(value.to_owned()),
        }
    }
}
