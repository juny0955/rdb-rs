use crate::sql::{
    Parser,
    ast::{Projection, SelectStatement, Statement},
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
            projections: vec![Projection::CountAll],
            table: "users".to_owned(),
            filter: None,
            order_by: None,
            limit: None,
        })
    );
}
