use crate::query::sql::{
    Parser,
    ast::{
        ColumnReference, Expression, FromClause, JoinCondition, Projection, SelectStatement,
        Statement, TableReference,
    },
    lexer::Lexer,
};

#[test]
fn select_join문을_ast로_파싱한다() {
    // Given
    let mut lexer =
        Lexer::new("SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![
                Projection::Expression(Expression::Column(ColumnReference {
                    table: Some("u".to_owned()),
                    column: "name".to_owned(),
                })),
                Projection::Expression(Expression::Column(ColumnReference {
                    table: Some("o".to_owned()),
                    column: "amount".to_owned(),
                })),
            ],
            from: FromClause::InnerJoin {
                left: Box::new(FromClause::Table(TableReference {
                    name: "users".to_owned(),
                    alias: Some("u".to_owned()),
                })),
                right: Box::new(FromClause::Table(TableReference {
                    name: "orders".to_owned(),
                    alias: Some("o".to_owned()),
                })),
                on: JoinCondition {
                    left: ColumnReference {
                        table: Some("u".to_owned()),
                        column: "id".to_owned(),
                    },
                    right: ColumnReference {
                        table: Some("o".to_owned()),
                        column: "user_id".to_owned(),
                    },
                },
            },
            filter: None,
            group_by: None,
            order_by: None,
            limit: None,
        })
    );
}
