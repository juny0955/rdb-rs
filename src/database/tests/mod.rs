use crate::{
    binder::{Binder, BoundCreateTable, BoundStatement},
    catalog::metadata::DatabaseMetadata,
    sql::{
        Parser,
        ast::{ColumnDefinition, DataType as AstDataType},
        lexer::Lexer,
    },
};

mod execute;
mod index;
mod table;

fn users_table() -> BoundCreateTable {
    BoundCreateTable {
        table: "users".to_owned(),
        columns: vec![
            ColumnDefinition {
                name: "id".to_owned(),
                data_type: AstDataType::BigInt,
            },
            ColumnDefinition {
                name: "name".to_owned(),
                data_type: AstDataType::Varchar,
            },
        ],
    }
}

fn bind_sql(sql: &str, metadata: &DatabaseMetadata) -> BoundStatement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    let statement = Parser::new(tokens).parse().expect("SQL을 파싱해야 함");
    Binder::new(metadata)
        .bind(&statement)
        .expect("SQL을 bind해야 함")
}

fn parse_sql(sql: &str) -> crate::sql::ast::Statement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}
