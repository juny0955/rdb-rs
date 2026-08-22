use crate::{
    parser::ast::{
        Assignment, CreateTableStatement, DataType as AstDataType, DeleteStatement, Expression,
        InsertStatement, Projection, SelectStatement, UpdateStatement,
    },
    schema::{ColumnId, ColumnMetadata, DataType, TableId, TableMetadata},
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
    DatabaseMetadata::new("mydb".to_owned(), vec![users]).expect("데이터베이스 생성 성공")
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
    DatabaseMetadata::new("mydb".to_owned(), vec![numbers]).expect("데이터베이스 생성 성공")
}

#[test]
fn 존재하는_테이블을_조회하는_statement를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::All],
        table: "users".to_owned(),
        filter: None,
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn 존재하지_않는_테이블을_조회하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::All],
        table: "orders".to_owned(),
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TableNotFound("orders".to_owned()))
    );
}

#[test]
fn 존재하는_projection_컬럼을_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::Expression(Expression::Identifier(
            "name".to_owned(),
        ))],
        table: "users".to_owned(),
        filter: None,
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn 존재하지_않는_projection_컬럼은_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::Expression(Expression::Identifier(
            "age".to_owned(),
        ))],
        table: "users".to_owned(),
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::ColumnNotFound {
            table: "users".to_owned(),
            column: "age".to_owned(),
        })
    );
}

#[test]
fn 이미_존재하는_테이블을_생성하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::CreateTable(CreateTableStatement {
        table: "users".to_owned(),
        columns: vec![],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::AlreadyExistsTable("users".to_owned()))
    );
}

#[test]
fn 컬럼_타입과_순서에_맞는_insert를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "users".to_owned(),
        literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn insert_값이_부족하면_값_개수_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "users".to_owned(),
        literals: vec![Literal::Integer(1)],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::ValueCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn insert_값이_초과하면_값_개수_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "users".to_owned(),
        literals: vec![
            Literal::Integer(1),
            Literal::String("Kim".to_owned()),
            Literal::Null,
        ],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::ValueCountMismatch {
            expected: 2,
            actual: 3,
        })
    );
}

#[test]
fn insert_값의_타입이_다르면_타입_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "users".to_owned(),
        literals: vec![
            Literal::String("one".to_owned()),
            Literal::String("Kim".to_owned()),
        ],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );
}

#[test]
fn 존재하지_않는_테이블에_insert하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "orders".to_owned(),
        literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TableNotFound("orders".to_owned()))
    );
}

#[test]
fn insert에서_null_값을_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "users".to_owned(),
        literals: vec![Literal::Null, Literal::Null],
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn int_최솟값을_insert한다() {
    let database = int_database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "numbers".to_owned(),
        literals: vec![Literal::Integer(i64::from(i32::MIN))],
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn int_최댓값을_insert한다() {
    let database = int_database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "numbers".to_owned(),
        literals: vec![Literal::Integer(i64::from(i32::MAX))],
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn int_최솟값보다_작은_값을_insert하면_타입_오류를_반환한다() {
    let database = int_database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "numbers".to_owned(),
        literals: vec![Literal::Integer(i64::from(i32::MIN) - 1)],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TypeMismatch {
            column: "value".to_owned(),
            expected: DataType::Int,
        })
    );
}

#[test]
fn int_최댓값보다_큰_값을_insert하면_타입_오류를_반환한다() {
    let database = int_database();
    let binder = Binder::new(&database);
    let statement = Statement::Insert(InsertStatement {
        table: "numbers".to_owned(),
        literals: vec![Literal::Integer(i64::from(i32::MAX) + 1)],
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TypeMismatch {
            column: "value".to_owned(),
            expected: DataType::Int,
        })
    );
}

#[test]
fn 컬럼_타입에_맞는_update를_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Update(UpdateStatement {
        table: "users".to_owned(),
        assignments: vec![Assignment {
            column: "name".to_owned(),
            value: Literal::String("Lee".to_owned()),
        }],
        filter: None,
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn 존재하지_않는_컬럼을_update하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Update(UpdateStatement {
        table: "users".to_owned(),
        assignments: vec![Assignment {
            column: "age".to_owned(),
            value: Literal::Integer(20),
        }],
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::ColumnNotFound {
            table: "users".to_owned(),
            column: "age".to_owned(),
        })
    );
}

#[test]
fn 컬럼_타입과_다른_값으로_update하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Update(UpdateStatement {
        table: "users".to_owned(),
        assignments: vec![Assignment {
            column: "id".to_owned(),
            value: Literal::String("one".to_owned()),
        }],
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );
}

#[test]
fn 존재하지_않는_테이블을_update하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Update(UpdateStatement {
        table: "orders".to_owned(),
        assignments: vec![Assignment {
            column: "name".to_owned(),
            value: Literal::String("Lee".to_owned()),
        }],
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TableNotFound("orders".to_owned()))
    );
}

#[test]
fn 컬럼_타입에_맞는_where_조건을_select에서_bind한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::All],
        table: "users".to_owned(),
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("id".to_owned())),
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        }),
    });

    assert!(binder.bind(&statement).is_ok());
}

#[test]
fn 컬럼_타입과_다른_where_조건을_update하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Update(UpdateStatement {
        table: "users".to_owned(),
        assignments: vec![Assignment {
            column: "name".to_owned(),
            value: Literal::String("Lee".to_owned()),
        }],
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("id".to_owned())),
            right: Box::new(Expression::Literal(Literal::String("one".to_owned()))),
        }),
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::TypeMismatch {
            column: "id".to_owned(),
            expected: DataType::BigInt,
        })
    );
}

#[test]
fn 존재하지_않는_where_컬럼으로_delete하면_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Delete(DeleteStatement {
        table: "users".to_owned(),
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("age".to_owned())),
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        }),
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::ColumnNotFound {
            table: "users".to_owned(),
            column: "age".to_owned(),
        })
    );
}

#[test]
fn 잘못된_형태의_where_조건은_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::All],
        table: "users".to_owned(),
        filter: Some(Expression::Literal(Literal::Integer(1))),
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::InvalidFilterExpression)
    );
}

#[test]
fn select을_bound_select으로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = SelectStatement {
        projections: vec![Projection::Expression(Expression::Identifier(
            "name".to_owned(),
        ))],
        table: "users".to_owned(),
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("id".to_owned())),
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        }),
    };

    assert_eq!(
        binder.bind_select(&statement),
        Ok(BoundSelect {
            table_id: TableId::new(1),
            projections: vec![BoundProjection::Column(ColumnId::new(2))],
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(1),
                value: Literal::Integer(1),
            }),
        })
    );
}

#[test]
fn 잘못된_projection_expression은_오류를_반환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = Statement::Select(SelectStatement {
        projections: vec![Projection::Expression(Expression::Literal(
            Literal::Integer(1),
        ))],
        table: "users".to_owned(),
        filter: None,
    });

    assert_eq!(
        binder.bind(&statement),
        Err(BinderError::InvalidProjectionExpression)
    );
}

#[test]
fn insert를_bound_insert로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = InsertStatement {
        table: "users".to_owned(),
        literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
    };

    assert_eq!(
        binder.bind_insert(&statement),
        Ok(BoundInsert {
            table_id: TableId::new(1),
            literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
        })
    );
}

#[test]
fn delete를_bound_delete로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = DeleteStatement {
        table: "users".to_owned(),
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("id".to_owned())),
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        }),
    };

    assert_eq!(
        binder.bind_delete(&statement),
        Ok(BoundDelete {
            table_id: TableId::new(1),
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(1),
                value: Literal::Integer(1),
            }),
        })
    );
}

#[test]
fn update를_bound_update로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let statement = UpdateStatement {
        table: "users".to_owned(),
        assignments: vec![Assignment {
            column: "name".to_owned(),
            value: Literal::String("Lee".to_owned()),
        }],
        filter: Some(Expression::Equal {
            left: Box::new(Expression::Identifier("id".to_owned())),
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        }),
    };

    assert_eq!(
        binder.bind_update(&statement),
        Ok(BoundUpdate {
            table_id: TableId::new(1),
            assignments: vec![BoundAssignment {
                column_id: ColumnId::new(2),
                value: Literal::String("Lee".to_owned()),
            }],
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(1),
                value: Literal::Integer(1),
            }),
        })
    );
}

#[test]
fn create_table을_bound_statement로변환한다() {
    let database = database();
    let binder = Binder::new(&database);
    let columns = vec![crate::parser::ast::ColumnDefinition {
        name: "id".to_owned(),
        data_type: AstDataType::BigInt,
    }];
    let statement = Statement::CreateTable(CreateTableStatement {
        table: "orders".to_owned(),
        columns: columns.clone(),
    });

    assert_eq!(
        binder.bind(&statement),
        Ok(BoundStatement::CreateTable(BoundCreateTable {
            table: "orders".to_owned(),
            columns,
        }))
    );
}
