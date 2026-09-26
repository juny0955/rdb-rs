use crate::query::sql::lexer::Lexer;

use super::*;

mod aggregate;
mod join;

fn column(name: &str) -> ColumnReference {
    ColumnReference {
        table: None,
        column: name.to_owned(),
    }
}

fn users_from() -> FromClause {
    FromClause::Table(TableReference {
        name: "users".to_owned(),
        alias: None,
    })
}

fn parse(sql: &str) -> Statement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}

fn select(projections: Vec<Projection>, filter: Option<Expression>) -> Statement {
    Statement::Select(SelectStatement {
        projections,
        from: users_from(),
        filter,
        group_by: None,
        order_by: None,
        limit: None,
    })
}

fn comparison(column_name: &str, operator: ComparisonOperator, value: Literal) -> Expression {
    Expression::Comparison {
        left: Box::new(Expression::Column(column(column_name))),
        operator,
        right: Box::new(Expression::Literal(value)),
    }
}

#[test]
fn select_where_문을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT name FROM users WHERE id = 10;"),
        select(
            vec![Projection::Expression(Expression::Column(column("name")))],
            Some(comparison(
                "id",
                ComparisonOperator::Equal,
                Literal::Integer(10)
            )),
        )
    );
}

#[test]
fn select_where_less_than_문을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users WHERE id < 10;"),
        select(
            vec![Projection::All],
            Some(comparison(
                "id",
                ComparisonOperator::LessThan,
                Literal::Integer(10),
            )),
        )
    );
}

#[test]
fn select_where_and_문을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users WHERE id = 1 AND name = 'Kim';"),
        select(
            vec![Projection::All],
            Some(Expression::And {
                left: Box::new(comparison(
                    "id",
                    ComparisonOperator::Equal,
                    Literal::Integer(1)
                )),
                right: Box::new(comparison(
                    "name",
                    ComparisonOperator::Equal,
                    Literal::String("Kim".to_owned()),
                )),
            }),
        )
    );
}

#[test]
fn select_where_연속_and_문을_왼쪽부터_중첩해_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users WHERE id = 1 AND name = 'Kim' AND id = 2;"),
        select(
            vec![Projection::All],
            Some(Expression::And {
                left: Box::new(Expression::And {
                    left: Box::new(comparison(
                        "id",
                        ComparisonOperator::Equal,
                        Literal::Integer(1),
                    )),
                    right: Box::new(comparison(
                        "name",
                        ComparisonOperator::Equal,
                        Literal::String("Kim".to_owned()),
                    )),
                }),
                right: Box::new(comparison(
                    "id",
                    ComparisonOperator::Equal,
                    Literal::Integer(2),
                )),
            }),
        )
    );
}

#[test]
fn select_where_or_문을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users WHERE id = 1 OR name = 'Kim';"),
        select(
            vec![Projection::All],
            Some(Expression::Or {
                left: Box::new(comparison(
                    "id",
                    ComparisonOperator::Equal,
                    Literal::Integer(1)
                )),
                right: Box::new(comparison(
                    "name",
                    ComparisonOperator::Equal,
                    Literal::String("Kim".to_owned()),
                )),
            }),
        )
    );
}

#[test]
fn select_where에서_and는_or보다_높은_우선순위를_가진다() {
    assert_eq!(
        parse("SELECT * FROM users WHERE id = 1 OR name = 'Kim' AND id = 2;"),
        select(
            vec![Projection::All],
            Some(Expression::Or {
                left: Box::new(comparison(
                    "id",
                    ComparisonOperator::Equal,
                    Literal::Integer(1)
                )),
                right: Box::new(Expression::And {
                    left: Box::new(comparison(
                        "name",
                        ComparisonOperator::Equal,
                        Literal::String("Kim".to_owned()),
                    )),
                    right: Box::new(comparison(
                        "id",
                        ComparisonOperator::Equal,
                        Literal::Integer(2)
                    )),
                }),
            }),
        )
    );
}

#[test]
fn select_order_by_방향이_없으면_asc로_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users ORDER BY name;"),
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            from: users_from(),
            filter: None,
            group_by: None,
            order_by: Some(OrderBy {
                column: column("name"),
                direction: SortDirection::Asc,
            }),
            limit: None,
        })
    );
}

#[test]
fn select_order_by_desc를_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT * FROM users ORDER BY name DESC;"),
        Statement::Select(SelectStatement {
            projections: vec![Projection::All],
            from: users_from(),
            filter: None,
            group_by: None,
            order_by: Some(OrderBy {
                column: column("name"),
                direction: SortDirection::Desc,
            }),
            limit: None,
        })
    );
}

#[test]
fn create_table_문을_ast로_파싱한다() {
    assert_eq!(
        parse("CREATE TABLE users (id BIGINT, name VARCHAR);"),
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
    assert_eq!(
        parse("CREATE INDEX idx_users_id ON users(id);"),
        Statement::CreateIndex(CreateIndexStatement {
            index_name: "idx_users_id".to_owned(),
            table: "users".to_owned(),
            column_name: "id".to_owned(),
        })
    );
}

#[test]
fn insert_문을_ast로_파싱한다() {
    assert_eq!(
        parse("INSERT INTO users VALUES (1, 'Kim', NULL);"),
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
    assert_eq!(
        parse("UPDATE users SET name = 'Lee' WHERE id = 1;"),
        Statement::Update(UpdateStatement {
            from: users_from(),
            assignments: vec![Assignment {
                column: column("name"),
                value: Literal::String("Lee".to_owned()),
            }],
            filter: Some(comparison(
                "id",
                ComparisonOperator::Equal,
                Literal::Integer(1)
            )),
        })
    );
}

#[test]
fn delete_문을_ast로_파싱한다() {
    assert_eq!(
        parse("DELETE FROM users WHERE id = 1;"),
        Statement::Delete(DeleteStatement {
            targets: None,
            from: users_from(),
            filter: Some(comparison(
                "id",
                ComparisonOperator::Equal,
                Literal::Integer(1)
            )),
        })
    );
}
