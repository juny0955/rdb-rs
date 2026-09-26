use crate::{
    catalog::metadata::{ColumnId, ColumnMetadata, DataType, TableId, TableMetadata},
    query::{
        common::{ComparisonOperator, SortDirection},
        sql::ast::{
            Aggregate, Assignment, ColumnDefinition, ColumnReference, CreateIndexStatement,
            CreateTableStatement, DeleteStatement, Expression, FromClause, InsertStatement,
            Literal, OrderBy, Projection, SelectStatement, SqlDataType, Statement, TableReference,
            UpdateStatement,
        },
    },
    tuple::Value,
};

use super::*;

fn database() -> DatabaseMetadata {
    let users = TableMetadata::new(
        TableId::new(1),
        "users".to_owned(),
        vec![
            ColumnMetadata::new(ColumnId::new(1), "id".to_owned(), DataType::BigInt),
            ColumnMetadata::new(ColumnId::new(2), "name".to_owned(), DataType::Varchar),
        ],
    )
    .expect("테이블 생성 성공");
    DatabaseMetadata::new("mydb".to_owned(), vec![users], vec![]).expect("데이터베이스 생성 성공")
}

fn int_database() -> DatabaseMetadata {
    let numbers = TableMetadata::new(
        TableId::new(2),
        "numbers".to_owned(),
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "value".to_owned(),
            DataType::Int,
        )],
    )
    .expect("테이블 생성 성공");
    DatabaseMetadata::new("mydb".to_owned(), vec![numbers], vec![]).expect("데이터베이스 생성 성공")
}

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

fn select(projections: Vec<Projection>) -> SelectStatement {
    SelectStatement {
        projections,
        from: from("users"),
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    }
}

fn comparison(name: &str, operator: ComparisonOperator, value: Literal) -> Expression {
    Expression::Comparison {
        left: Box::new(Expression::Column(column(name))),
        operator,
        right: Box::new(Expression::Literal(value)),
    }
}

fn users_table() -> BoundTable {
    BoundTable {
        table_id: TableId::new(1),
        instance_id: TableInstanceId(0),
        alias: None,
    }
}

fn users_column(column_id: u32) -> BoundColumnReference {
    BoundColumnReference {
        table_id: TableId::new(1),
        instance_id: TableInstanceId(0),
        column_id: ColumnId::new(column_id),
    }
}

#[test]
fn select의_테이블과_projection을_bind한다() {
    let database = database();
    let binder = Binder::new(&database);

    let bound = binder
        .bind(&Statement::Select(select(vec![
            Projection::All,
            Projection::Expression(Expression::Column(column("name"))),
        ])))
        .expect("SELECT를 bind해야 함");

    assert_eq!(
        bound,
        BoundStatement::Select(BoundSelect {
            projections: vec![
                BoundProjection::All,
                BoundProjection::Column(users_column(2))
            ],
            from: BoundFromClause::Table(users_table()),
            filter: None,
            group_by: None,
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn 존재하지_않는_select_테이블과_컬럼을_거부한다() {
    let database = database();
    let binder = Binder::new(&database);
    let mut missing_table = select(vec![Projection::All]);
    missing_table.from = from("orders");

    assert_eq!(
        binder.bind(&Statement::Select(missing_table)),
        Err(BinderError::TableNotFound("orders".to_owned()))
    );
    assert_eq!(
        binder.bind(&Statement::Select(select(vec![Projection::Expression(
            Expression::Column(column("age")),
        )]))),
        Err(BinderError::ColumnNotFoundInScope {
            column: "age".to_owned(),
        })
    );
}

#[test]
fn sum과_group_by를_bound_column_reference로_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let mut statement = select(vec![
        Projection::Expression(Expression::Column(column("name"))),
        Projection::Aggregate(Aggregate::Sum(column("id"))),
    ]);
    statement.group_by = Some(vec![column("name")]);

    assert_eq!(
        binder.bind_select(&statement),
        Ok(BoundSelect {
            projections: vec![
                BoundProjection::Column(users_column(2)),
                BoundProjection::Aggregate(BoundAggregate::Sum(users_column(1))),
            ],
            from: BoundFromClause::Table(users_table()),
            filter: None,
            group_by: Some(vec![users_column(2)]),
            order_by: None,
            limit: None,
        })
    );
}

#[test]
fn sum과_group_by의_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);

    assert_eq!(
        binder.bind_select(&select(vec![Projection::Aggregate(Aggregate::Sum(
            column("name",)
        ))])),
        Err(BinderError::UnsupportedAggregateType)
    );
    assert_eq!(
        binder.bind_select(&select(vec![Projection::Aggregate(Aggregate::Sum(
            column("age",)
        ))])),
        Err(BinderError::ColumnNotFoundInScope {
            column: "age".to_owned(),
        })
    );

    let mut invalid_group = select(vec![Projection::Aggregate(Aggregate::CountAll)]);
    invalid_group.group_by = Some(vec![column("age")]);
    assert_eq!(
        binder.bind_select(&invalid_group),
        Err(BinderError::ColumnNotFoundInScope {
            column: "age".to_owned(),
        })
    );
}

#[test]
fn aggregate_projection은_group_by_규칙을지킨다() {
    let database = database();
    let binder = Binder::new(&database);
    let mut without_group = select(vec![
        Projection::Expression(Expression::Column(column("name"))),
        Projection::Aggregate(Aggregate::CountAll),
    ]);
    assert_eq!(
        binder.bind_select(&without_group),
        Err(BinderError::InvalidGroupingProjection)
    );

    without_group.group_by = Some(vec![column("id")]);
    assert_eq!(
        binder.bind_select(&without_group),
        Err(BinderError::InvalidGroupingProjection)
    );
}

#[test]
fn insert의_값_개수와_타입을검증한다() {
    let database = database();
    let binder = Binder::new(&database);

    assert_eq!(
        binder.bind_insert(&InsertStatement {
            table: "users".to_owned(),
            literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
        }),
        Ok(BoundInsert {
            table_id: TableId::new(1),
            values: vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        })
    );
    assert_eq!(
        binder.bind_insert(&InsertStatement {
            table: "users".to_owned(),
            literals: vec![Literal::Integer(1)],
        }),
        Err(BinderError::ValueCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
    assert_eq!(
        binder.bind_insert(&InsertStatement {
            table: "users".to_owned(),
            literals: vec![
                Literal::String("one".to_owned()),
                Literal::String("Kim".to_owned()),
            ],
        }),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );
}

#[test]
fn insert의_int_범위와_null을검증한다() {
    let int_database = int_database();
    let binder = Binder::new(&int_database);

    for value in [i64::from(i32::MIN), i64::from(i32::MAX)] {
        assert!(
            binder
                .bind_insert(&InsertStatement {
                    table: "numbers".to_owned(),
                    literals: vec![Literal::Integer(value)],
                })
                .is_ok()
        );
    }
    for value in [i64::from(i32::MIN) - 1, i64::from(i32::MAX) + 1] {
        assert_eq!(
            binder.bind_insert(&InsertStatement {
                table: "numbers".to_owned(),
                literals: vec![Literal::Integer(value)],
            }),
            Err(BinderError::TypeMismatch {
                column: "value".to_owned(),
                expected: DataType::Int,
            })
        );
    }

    let database = database();
    assert!(
        Binder::new(&database)
            .bind_insert(&InsertStatement {
                table: "users".to_owned(),
                literals: vec![Literal::Null, Literal::Null],
            })
            .is_ok()
    );
}

#[test]
fn update는_qualified_column과_filter를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = UpdateStatement {
        from: from("users"),
        assignments: vec![Assignment {
            column: column("name"),
            value: Literal::String("Lee".to_owned()),
        }],
        filter: Some(comparison(
            "id",
            ComparisonOperator::Equal,
            Literal::Integer(1),
        )),
    };

    assert_eq!(
        binder.bind_update(&statement),
        Ok(BoundUpdate {
            from: BoundFromClause::Table(users_table()),
            assignments: vec![BoundAssignment {
                column: users_column(2),
                value: Value::Varchar("Lee".to_owned()),
            }],
            filter: Some(BoundExpression::Comparison {
                column: users_column(1),
                operator: ComparisonOperator::Equal,
                value: Value::BigInt(1),
            }),
        })
    );
}

#[test]
fn update의_테이블_컬럼_타입_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let invalid_column = UpdateStatement {
        from: from("users"),
        assignments: vec![Assignment {
            column: column("age"),
            value: Literal::Integer(20),
        }],
        filter: None,
    };
    assert_eq!(
        binder.bind_update(&invalid_column),
        Err(BinderError::ColumnNotFoundInScope {
            column: "age".to_owned(),
        })
    );

    let invalid_value = UpdateStatement {
        from: from("users"),
        assignments: vec![Assignment {
            column: column("id"),
            value: Literal::String("one".to_owned()),
        }],
        filter: None,
    };
    assert_eq!(
        binder.bind_update(&invalid_value),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );
}

#[test]
fn filter의_컬럼과_타입을검증한다() {
    let database = database();
    let binder = Binder::new(&database);
    let mut statement = select(vec![Projection::All]);
    statement.filter = Some(comparison(
        "id",
        ComparisonOperator::Equal,
        Literal::String("one".to_owned()),
    ));
    assert_eq!(
        binder.bind_select(&statement),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );

    statement.filter = Some(comparison(
        "age",
        ComparisonOperator::Equal,
        Literal::Integer(1),
    ));
    assert_eq!(
        binder.bind_select(&statement),
        Err(BinderError::ColumnNotFoundInScope {
            column: "age".to_owned(),
        })
    );

    statement.filter = Some(Expression::Literal(Literal::Integer(1)));
    assert_eq!(
        binder.bind_select(&statement),
        Err(BinderError::InvalidFilterExpression)
    );
}

#[test]
fn and와_or_filter를_bound_expression으로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    for filter in [
        Expression::And {
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
        },
        Expression::Or {
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
        },
    ] {
        let mut statement = select(vec![Projection::All]);
        statement.filter = Some(filter);
        assert!(binder.bind_select(&statement).is_ok());
    }
}

#[test]
fn order_by는_group_by와_aggregate_규칙을검증한다() {
    let database = database();
    let binder = Binder::new(&database);
    let mut statement = select(vec![
        Projection::Expression(Expression::Column(column("name"))),
        Projection::Aggregate(Aggregate::CountAll),
    ]);
    statement.group_by = Some(vec![column("name")]);
    statement.order_by = Some(OrderBy {
        column: column("name"),
        direction: SortDirection::Asc,
    });
    assert!(binder.bind_select(&statement).is_ok());

    statement.order_by = Some(OrderBy {
        column: column("id"),
        direction: SortDirection::Asc,
    });
    assert_eq!(
        binder.bind_select(&statement),
        Err(BinderError::InvalidGroupingOrder)
    );

    statement.group_by = None;
    statement.projections = vec![Projection::Aggregate(Aggregate::CountAll)];
    assert_eq!(
        binder.bind_select(&statement),
        Err(BinderError::InvalidAggregateOrder)
    );
}

#[test]
fn delete는_leftmost_table과_filter를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = DeleteStatement {
        targets: None,
        from: from("users"),
        filter: Some(comparison(
            "id",
            ComparisonOperator::Equal,
            Literal::Integer(1),
        )),
    };

    assert_eq!(
        binder.bind_delete(&statement),
        Ok(BoundDelete {
            targets: vec![users_table()],
            from: BoundFromClause::Table(users_table()),
            filter: Some(BoundExpression::Comparison {
                column: users_column(1),
                operator: ComparisonOperator::Equal,
                value: Value::BigInt(1),
            }),
        })
    );
}

#[test]
fn create_table과_index를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    assert_eq!(
        binder.bind(&Statement::CreateTable(CreateTableStatement {
            table: "orders".to_owned(),
            columns: vec![ColumnDefinition {
                name: "id".to_owned(),
                data_type: SqlDataType::BigInt,
            }],
        })),
        Ok(BoundStatement::CreateTable(BoundCreateTable {
            table: "orders".to_owned(),
            columns: vec![BoundColumnDefinition {
                name: "id".to_owned(),
                data_type: DataType::BigInt,
            }],
        }))
    );
    assert_eq!(
        binder.bind(&Statement::CreateIndex(CreateIndexStatement {
            index_name: "idx_users_id".to_owned(),
            table: "users".to_owned(),
            column_name: "id".to_owned(),
        })),
        Ok(BoundStatement::CreateIndex(BoundCreateIndex {
            index_name: "idx_users_id".to_owned(),
            table_id: TableId::new(1),
            column_id: ColumnId::new(1),
        }))
    );
}

#[test]
fn 존재하지_않는_create_대상을_거부한다() {
    let database = database();
    let binder = Binder::new(&database);
    assert_eq!(
        binder.bind(&Statement::CreateTable(CreateTableStatement {
            table: "users".to_owned(),
            columns: vec![],
        })),
        Err(BinderError::AlreadyExistsTable("users".to_owned()))
    );
    assert_eq!(
        binder.bind(&Statement::CreateIndex(CreateIndexStatement {
            index_name: "idx_users_age".to_owned(),
            table: "users".to_owned(),
            column_name: "age".to_owned(),
        })),
        Err(BinderError::ColumnNotFound {
            table: "users".to_owned(),
            column: "age".to_owned(),
        })
    );
}
