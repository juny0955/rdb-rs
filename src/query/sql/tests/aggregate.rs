use crate::query::sql::{
    Parser,
    ast::{
        Aggregate, ColumnReference, Expression, FromClause, Projection, SelectStatement, Statement,
        TableReference,
    },
    lexer::Lexer,
};

fn column(name: &str) -> ColumnReference {
    ColumnReference {
        table: None,
        column: name.to_owned(),
    }
}

fn from(table: &str) -> FromClause {
    FromClause::Table(TableReference {
        name: table.to_owned(),
        alias: None,
    })
}

fn parse(sql: &str) -> Statement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}

#[test]
fn select_count_all을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT COUNT(*) FROM users;"),
        Statement::Select(SelectStatement {
            projections: vec![Projection::Aggregate(Aggregate::CountAll)],
            from: from("users"),
            filter: None,
            group_by: None,
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_sum을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT SUM(id) FROM users;"),
        Statement::Select(SelectStatement {
            projections: vec![Projection::Aggregate(Aggregate::Sum(column("id")))],
            from: from("users"),
            filter: None,
            group_by: None,
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_group_by_단일_컬럼을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT department_id, COUNT(*) FROM employees GROUP BY department_id;"),
        Statement::Select(SelectStatement {
            projections: vec![
                Projection::Expression(Expression::Column(column("department_id"))),
                Projection::Aggregate(Aggregate::CountAll),
            ],
            from: from("employees"),
            filter: None,
            group_by: Some(vec![column("department_id")]),
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn select_group_by_복수_컬럼을_ast로_파싱한다() {
    assert_eq!(
        parse("SELECT department_id, role, COUNT(*) FROM employees GROUP BY department_id, role;"),
        Statement::Select(SelectStatement {
            projections: vec![
                Projection::Expression(Expression::Column(column("department_id"))),
                Projection::Expression(Expression::Column(column("role"))),
                Projection::Aggregate(Aggregate::CountAll),
            ],
            from: from("employees"),
            filter: None,
            group_by: Some(vec![column("department_id"), column("role")]),
            order_by: None,
            limit: None,
        })
    );
}
