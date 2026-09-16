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
            group_by: None,
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
            group_by: None,
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_group_by_단일_컬럼을_ast로_파싱한다() {
    // Given
    let mut lexer =
        Lexer::new("SELECT department_id, COUNT(*) FROM employees GROUP BY department_id;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![
                Projection::Expression(crate::query::sql::ast::Expression::Identifier(
                    "department_id".to_owned(),
                )),
                Projection::Aggregate(Aggregate::CountAll),
            ],
            table: "employees".to_owned(),
            filter: None,
            group_by: Some(vec!["department_id".to_owned()]),
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_group_by_복수_컬럼을_ast로_파싱한다() {
    // Given
    let mut lexer = Lexer::new(
        "SELECT department_id, role, COUNT(*) FROM employees GROUP BY department_id, role;",
    );
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![
                Projection::Expression(crate::query::sql::ast::Expression::Identifier(
                    "department_id".to_owned(),
                )),
                Projection::Expression(crate::query::sql::ast::Expression::Identifier(
                    "role".to_owned(),
                )),
                Projection::Aggregate(Aggregate::CountAll),
            ],
            table: "employees".to_owned(),
            filter: None,
            group_by: Some(vec!["department_id".to_owned(), "role".to_owned()]),
            order_by: None,
            limit: None,
        })
    );
}
