use crate::sql::lexer::Lexer;

use super::*;

#[test]
fn select_where_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("SELECT name FROM users WHERE id = 10;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::Expression(Expression::Identifier(
                "name".to_owned()
            ))],
            table: "users".to_owned(),
            filter: Some(Expression::Comparison {
                left: Box::new(Expression::Identifier("id".to_owned())),
                operator: ComparisonOperator::Equal,
                right: Box::new(Expression::Literal(Literal::Integer(10))),
            }),
            order_by: None,
        })
    );
}

#[test]
fn select_where_less_than_문을_ast로_파싱한다() {
    // Given
    let mut lexer = Lexer::new("SELECT * FROM users WHERE id < 10;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: Some(Expression::Comparison {
                left: Box::new(Expression::Identifier("id".to_owned())),
                operator: ComparisonOperator::LessThan,
                right: Box::new(Expression::Literal(Literal::Integer(10))),
            }),
            order_by: None,
        })
    );
}

#[test]
fn select_where_and_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("SELECT * FROM users WHERE id = 1 AND name = 'Kim';");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: Some(Expression::And {
                left: Box::new(Expression::Comparison {
                    left: Box::new(Expression::Identifier("id".to_owned())),
                    operator: ComparisonOperator::Equal,
                    right: Box::new(Expression::Literal(Literal::Integer(1))),
                }),
                right: Box::new(Expression::Comparison {
                    left: Box::new(Expression::Identifier("name".to_owned())),
                    operator: ComparisonOperator::Equal,
                    right: Box::new(Expression::Literal(Literal::String("Kim".to_owned()))),
                }),
            }),
            order_by: None,
        })
    );
}

#[test]
fn select_where_or_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("SELECT * FROM users WHERE id = 1 OR name = 'Kim';");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: Some(Expression::Or {
                left: Box::new(Expression::Comparison {
                    left: Box::new(Expression::Identifier("id".to_owned())),
                    operator: ComparisonOperator::Equal,
                    right: Box::new(Expression::Literal(Literal::Integer(1))),
                }),
                right: Box::new(Expression::Comparison {
                    left: Box::new(Expression::Identifier("name".to_owned())),
                    operator: ComparisonOperator::Equal,
                    right: Box::new(Expression::Literal(Literal::String("Kim".to_owned()))),
                }),
            }),
            order_by: None,
        })
    );
}

#[test]
fn select_where에서_and는_or보다_높은_우선순위를_가진다() {
    let mut lexer = Lexer::new("SELECT * FROM users WHERE id = 1 OR name = 'Kim' AND id = 2;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: Some(Expression::Or {
                left: Box::new(Expression::Comparison {
                    left: Box::new(Expression::Identifier("id".to_owned())),
                    operator: ComparisonOperator::Equal,
                    right: Box::new(Expression::Literal(Literal::Integer(1))),
                }),
                right: Box::new(Expression::And {
                    left: Box::new(Expression::Comparison {
                        left: Box::new(Expression::Identifier("name".to_owned())),
                        operator: ComparisonOperator::Equal,
                        right: Box::new(Expression::Literal(Literal::String("Kim".to_owned()))),
                    }),
                    right: Box::new(Expression::Comparison {
                        left: Box::new(Expression::Identifier("id".to_owned())),
                        operator: ComparisonOperator::Equal,
                        right: Box::new(Expression::Literal(Literal::Integer(2))),
                    }),
                }),
            }),
            order_by: None,
        })
    );
}

#[test]
fn select_order_by_방향이_없으면_asc로_파싱한다() {
    // Given
    let mut lexer = Lexer::new("SELECT * FROM users ORDER BY name;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: None,
            order_by: Some(OrderBy {
                column: "name".to_owned(),
                direction: SortDirection::Asc,
            }),
        })
    );
}

#[test]
fn select_order_by_desc를_ast로_파싱한다() {
    // Given
    let mut lexer = Lexer::new("SELECT * FROM users ORDER BY name DESC;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    // When
    let statement = parser.parse().unwrap();

    // Then
    assert_eq!(
        statement,
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            table: "users".to_owned(),
            filter: None,
            order_by: Some(OrderBy {
                column: "name".to_owned(),
                direction: SortDirection::Desc,
            }),
        })
    );
}

#[test]
fn create_table_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("CREATE TABLE users (id BIGINT, name VARCHAR);");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::CreateTable(CreateTableStatement {
            table: "users".to_owned(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_owned(),
                    data_type: SqlDataType::BigInt,
                },
                ColumnDefinition {
                    name: "name".to_owned(),
                    data_type: SqlDataType::Varchar,
                },
            ],
        })
    );
}

#[test]
fn create_index_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("CREATE INDEX idx_users_id ON users(id);");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::CreateIndex(CreateIndexStatement {
            index_name: "idx_users_id".to_owned(),
            table: "users".to_owned(),
            column_name: "id".to_owned(),
        })
    );
}

#[test]
fn insert_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("INSERT INTO users VALUES (1, 'Kim', NULL);");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Insert(InsertStatement {
            table: "users".to_owned(),
            literals: vec![
                Literal::Integer(1),
                Literal::String("Kim".to_owned()),
                Literal::Null,
            ],
        })
    );
}

#[test]
fn update_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("UPDATE users SET name = 'Lee' WHERE id = 1;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Update(UpdateStatement {
            table: "users".to_owned(),
            assignments: vec![Assignment {
                column: "name".to_owned(),
                value: Literal::String("Lee".to_owned()),
            }],
            filter: Some(Expression::Comparison {
                left: Box::new(Expression::Identifier("id".to_owned())),
                operator: ComparisonOperator::Equal,
                right: Box::new(Expression::Literal(Literal::Integer(1))),
            }),
        })
    );
}

#[test]
fn delete_문을_ast로_파싱한다() {
    let mut lexer = Lexer::new("DELETE FROM users WHERE id = 1;");
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let statement = parser.parse().unwrap();

    assert_eq!(
        statement,
        Statement::Delete(DeleteStatement {
            table: "users".to_owned(),
            filter: Some(Expression::Comparison {
                left: Box::new(Expression::Identifier("id".to_owned())),
                operator: ComparisonOperator::Equal,
                right: Box::new(Expression::Literal(Literal::Integer(1))),
            }),
        })
    );
}
