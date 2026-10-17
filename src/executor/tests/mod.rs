use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata,
    },
    query::{
        binder::{
            BoundAssignment, BoundColumnReference, BoundDelete, BoundExpression, BoundFromClause,
            BoundJoinCondition, BoundOrderBy, BoundProjection, BoundSelect, BoundTable,
            BoundUpdate, TableInstanceId,
        },
        common::{ComparisonOperator, SortDirection},
    },
    storage::page::{PageId, RowId, SlotId},
    tuple::Value,
};

use super::{
    Executor, ExecutorError, QueryRow, TableRow,
    state::{
        delete::DeleteState,
        select::{SelectState, SelectStep},
    },
};

mod aggregate;
mod select;

fn collect_select(
    executor: &Executor<'_>,
    rows: Vec<QueryRow>,
    bound: BoundSelect,
) -> Result<Vec<Vec<Value>>, ExecutorError> {
    let mut state = SelectState::new(bound)?;
    let mut results = Vec::new();
    for row in rows {
        if state.is_done() {
            break;
        }
        match state.push_row(executor, &row)? {
            SelectStep::Row(values) => results.push(values),
            SelectStep::NeedInput => {}
            SelectStep::Done => break,
        }
    }
    loop {
        match state.finish(executor)? {
            SelectStep::Row(values) => results.push(values),
            SelectStep::Done => return Ok(results),
            SelectStep::NeedInput => panic!("EOF 후에는 추가 입력을 요구하면 안 됨"),
        }
    }
}

fn users_columns() -> Vec<ColumnMetadata> {
    vec![
        ColumnMetadata::new(ColumnId::new(1), "id".to_owned(), DataType::BigInt),
        ColumnMetadata::new(ColumnId::new(2), "name".to_owned(), DataType::Varchar),
    ]
}

fn database(table_id: TableId) -> DatabaseMetadata {
    let table = TableMetadata::new(table_id, "users".to_owned(), users_columns())
        .expect("테이블 메타데이터가 유효해야 함");
    DatabaseMetadata::new("test".to_owned(), vec![table], vec![])
        .expect("데이터베이스 메타데이터가 유효해야 함")
}

fn users_table(table_id: TableId) -> BoundTable {
    BoundTable {
        table_id,
        instance_id: TableInstanceId(0),
        alias: None,
    }
}

fn users_from(table_id: TableId) -> BoundFromClause {
    BoundFromClause::Table(users_table(table_id))
}

fn users_column(table_id: TableId, column_id: u32) -> BoundColumnReference {
    BoundColumnReference {
        table_id,
        instance_id: TableInstanceId(0),
        column_id: ColumnId::new(column_id),
    }
}

fn row(slot: u16, id: Value, name: Value) -> QueryRow {
    QueryRow {
        table_rows: vec![TableRow {
            row_id: RowId::new(PageId::new(1), SlotId::new(slot)),
            instance_id: TableInstanceId(0),
            values: vec![id, name],
        }],
    }
}

fn rows() -> Vec<QueryRow> {
    vec![
        row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned())),
        row(2, Value::BigInt(2), Value::Varchar("Lee".to_owned())),
        row(3, Value::Null, Value::Varchar("Null".to_owned())),
    ]
}

fn select(table_id: TableId, projections: Vec<BoundProjection>) -> BoundSelect {
    BoundSelect {
        projections,
        from: users_from(table_id),
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    }
}

fn comparison(
    table_id: TableId,
    column_id: u32,
    operator: ComparisonOperator,
    value: Value,
) -> BoundExpression {
    BoundExpression::Comparison {
        column: users_column(table_id, column_id),
        operator,
        value,
    }
}

#[test]
fn insert는_리터럴을_row로_변환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = crate::query::binder::BoundInsert {
        table_id,
        values: vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
    };

    assert!(Executor::new(&database).encode_insert(&bound).is_ok());
}

#[test]
fn update는_filter와_일치하는_query_row만_준비한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = BoundUpdate {
        from: users_from(table_id),
        assignments: vec![BoundAssignment {
            column: users_column(table_id, 2),
            value: Value::Varchar("Park".to_owned()),
        }],
        filter: Some(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(1),
        )),
    };

    let prepared = Executor::new(&database)
        .prepare_update(rows(), &bound)
        .expect("UPDATE 대상을 준비해야 함");

    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].new_values,
        vec![Value::BigInt(1), Value::Varchar("Park".to_owned())]
    );
}

#[test]
fn delete는_filter와_일치하는_query_row만_준비한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = BoundDelete {
        targets: vec![users_table(table_id)],
        from: users_from(table_id),
        filter: Some(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(2),
        )),
    };

    let executor = Executor::new(&database);
    let mut state = DeleteState::new(bound);
    let mut prepared = Vec::new();
    for row in rows() {
        prepared.extend(
            state
                .prepare_row(&executor, &row)
                .expect("DELETE 대상을 준비해야 함"),
        );
    }

    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].values,
        vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())]
    );
    assert_eq!(
        state.into_targets(),
        [(table_id, RowId::new(PageId::new(1), SlotId::new(2)))].into()
    );
}

#[test]
fn delete_state는_다른_입력과_별칭의_동일한_물리행을_한번만_준비한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let first_target = users_table(table_id);
    let mut second_target = first_target.clone();
    second_target.instance_id = TableInstanceId(1);
    second_target.alias = Some("other".to_owned());
    let mut state = DeleteState::new(BoundDelete {
        from: BoundFromClause::InnerJoin {
            left: Box::new(BoundFromClause::Table(first_target.clone())),
            right: Box::new(BoundFromClause::Table(second_target.clone())),
            on: BoundJoinCondition {
                left: users_column(table_id, 1),
                right: BoundColumnReference {
                    instance_id: second_target.instance_id,
                    ..users_column(table_id, 1)
                },
            },
        },
        targets: vec![first_target, second_target],
        filter: None,
    });
    let mut input = row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned()));
    let mut alias_row = input.table_rows[0].clone();
    alias_row.instance_id = TableInstanceId(1);
    input.table_rows.push(alias_row);

    let prepared = state
        .prepare_row(&executor, &input)
        .expect("첫 입력에서 삭제 대상을 준비해야 함");
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].table_id, table_id);
    assert_eq!(prepared[0].row_id, input.table_rows[0].row_id);
    assert!(
        state
            .prepare_row(&executor, &input)
            .expect("반복 입력은 오류 없이 처리해야 함")
            .is_empty()
    );
    assert_eq!(
        state.into_targets(),
        [(table_id, input.table_rows[0].row_id)].into()
    );
}

#[test]
fn delete_state는_필터_불일치_대상을_seen에_기록하지_않는다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut other = users_table(table_id);
    other.instance_id = TableInstanceId(1);
    other.alias = Some("other".to_owned());
    let mut state = DeleteState::new(BoundDelete {
        targets: vec![users_table(table_id)],
        from: BoundFromClause::InnerJoin {
            left: Box::new(users_from(table_id)),
            right: Box::new(BoundFromClause::Table(other.clone())),
            on: BoundJoinCondition {
                left: users_column(table_id, 2),
                right: BoundColumnReference {
                    instance_id: other.instance_id,
                    ..users_column(table_id, 2)
                },
            },
        },
        filter: Some(BoundExpression::Comparison {
            column: BoundColumnReference {
                instance_id: other.instance_id,
                ..users_column(table_id, 1)
            },
            operator: ComparisonOperator::Equal,
            value: Value::BigInt(2),
        }),
    });
    let mut rejected = row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned()));
    rejected.table_rows.push(TableRow {
        row_id: RowId::new(PageId::new(1), SlotId::new(3)),
        instance_id: other.instance_id,
        values: vec![Value::BigInt(3), Value::Varchar("Kim".to_owned())],
    });
    let mut accepted = rejected.clone();
    accepted.table_rows[1].row_id = RowId::new(PageId::new(1), SlotId::new(2));
    accepted.table_rows[1].values[0] = Value::BigInt(2);

    assert!(
        state
            .prepare_row(&executor, &rejected)
            .expect("필터 불일치는 오류 없이 처리해야 함")
            .is_empty()
    );
    let prepared = state
        .prepare_row(&executor, &accepted)
        .expect("이전에 제외된 식별자도 조건에 맞으면 준비해야 함");
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].values, accepted.table_rows[0].values);
    assert_eq!(
        state.into_targets(),
        [(table_id, accepted.table_rows[0].row_id)].into()
    );
}

#[test]
fn delete_state는_row_id가_같아도_테이블이_다르면_구분한다() {
    let first_table_id = TableId::new(1);
    let second_table_id = TableId::new(2);
    let database = DatabaseMetadata::new(
        "test".to_owned(),
        vec![
            TableMetadata::new(first_table_id, "users".to_owned(), users_columns())
                .expect("첫 테이블 메타데이터가 유효해야 함"),
            TableMetadata::new(second_table_id, "others".to_owned(), users_columns())
                .expect("두 번째 테이블 메타데이터가 유효해야 함"),
        ],
        vec![],
    )
    .expect("서로 다른 두 테이블이 유효해야 함");
    let executor = Executor::new(&database);
    let mut second_target = users_table(second_table_id);
    second_target.instance_id = TableInstanceId(1);
    let mut state = DeleteState::new(BoundDelete {
        from: BoundFromClause::InnerJoin {
            left: Box::new(users_from(first_table_id)),
            right: Box::new(BoundFromClause::Table(second_target.clone())),
            on: BoundJoinCondition {
                left: users_column(first_table_id, 1),
                right: BoundColumnReference {
                    instance_id: second_target.instance_id,
                    ..users_column(second_table_id, 1)
                },
            },
        },
        targets: vec![users_table(first_table_id), second_target],
        filter: None,
    });
    let mut input = row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned()));
    let mut second_row = input.table_rows[0].clone();
    second_row.instance_id = TableInstanceId(1);
    input.table_rows.push(second_row);

    let prepared = state
        .prepare_row(&executor, &input)
        .expect("서로 다른 테이블의 행을 각각 준비해야 함");
    assert_eq!(prepared.len(), 2);
    assert_eq!(
        state.into_targets(),
        [
            (first_table_id, input.table_rows[0].row_id),
            (second_table_id, input.table_rows[0].row_id),
        ]
        .into()
    );
}

#[test]
fn select는_all과_column_projection을반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);

    assert_eq!(
        collect_select(
            &executor,
            rows(),
            select(table_id, vec![BoundProjection::All])
        )
        .expect("SELECT가 성공해야 함"),
        vec![
            vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
            vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())],
            vec![Value::Null, Value::Varchar("Null".to_owned())],
        ]
    );
    assert_eq!(
        collect_select(
            &executor,
            rows(),
            select(
                table_id,
                vec![BoundProjection::Column(users_column(table_id, 2))]
            ),
        )
        .expect("SELECT가 성공해야 함"),
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
            vec![Value::Varchar("Null".to_owned())],
        ]
    );
}

#[test]
fn select는_comparison_and_or_filter를적용한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 2))],
    );
    bound.filter = Some(BoundExpression::And {
        left: Box::new(comparison(
            table_id,
            1,
            ComparisonOperator::GreaterThan,
            Value::BigInt(0),
        )),
        right: Box::new(comparison(
            table_id,
            2,
            ComparisonOperator::Equal,
            Value::Varchar("Kim".to_owned()),
        )),
    });
    assert_eq!(
        collect_select(&executor, rows(), bound).expect("AND filter가 성공해야 함"),
        vec![vec![Value::Varchar("Kim".to_owned())]]
    );

    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 2))],
    );
    bound.filter = Some(BoundExpression::Or {
        left: Box::new(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(2),
        )),
        right: Box::new(comparison(
            table_id,
            2,
            ComparisonOperator::Equal,
            Value::Varchar("Kim".to_owned()),
        )),
    });
    assert_eq!(
        collect_select(&executor, rows(), bound).expect("OR filter가 성공해야 함"),
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
}

#[test]
fn select의_null_comparison은_일치하지않는다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(table_id, vec![BoundProjection::All]);
    bound.filter = Some(comparison(
        table_id,
        1,
        ComparisonOperator::Equal,
        Value::Null,
    ));

    assert!(
        collect_select(&Executor::new(&database), rows(), bound)
            .expect("NULL filter가 성공해야 함")
            .is_empty()
    );
}

#[test]
fn select는_order_by와_limit을적용한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 2))],
    );
    bound.order_by = Some(BoundOrderBy {
        column: users_column(table_id, 1),
        direction: SortDirection::Desc,
    });
    bound.limit = Some(2);

    assert_eq!(
        collect_select(&Executor::new(&database), rows(), bound)
            .expect("ORDER BY와 LIMIT이 성공해야 함"),
        vec![
            vec![Value::Varchar("Null".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
}
