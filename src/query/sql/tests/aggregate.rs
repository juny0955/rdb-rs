use crate::query::sql::{
    Parser,
    ast::{Aggregate, Projection, SelectStatement, Statement},
    lexer::Lexer,
};

#[test]
fn select_count_all을_ast로_파싱한다() {
    // Given
    let mut lexer = Lexer::new("SELECT COUNT(*) FROM users;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::Aggregate(Aggregate::CountAll)],
            table: "users".to_owned(),
            filter: None,
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_sum을_ast로_파싱한다() {
    // Given
    let mut lexer = Lexer::new("SELECT SUM(id) FROM users;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::Aggregate(Aggregate::Sum("id".to_owned()))],
            table: "users".to_owned(),
            filter: None,
            order_by: None,
            limit: None,
        })
    );
}
