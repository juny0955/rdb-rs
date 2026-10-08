use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata,
    },
    executor::{Executor, ExecutorError, QueryRow, TableRow},
    query::binder::{BoundAggregate, BoundColumnReference, BoundProjection, TableInstanceId},
    storage::page::{PageId, RowId, SlotId},
    tuple::Value,
};

use super::GroupState;

fn database() -> DatabaseMetadata {
    let table = TableMetadata::new(
        TableId::new(1),
        "items".to_owned(),
        vec![
            ColumnMetadata::new(ColumnId::new(1), "amount".to_owned(), DataType::BigInt),
            ColumnMetadata::new(ColumnId::new(2), "category".to_owned(), DataType::Varchar),
        ],
    )
    .expect("테이블 메타데이터가 유효해야 함");
    DatabaseMetadata::new("test".to_owned(), vec![table], vec![])
        .expect("데이터베이스 메타데이터가 유효해야 함")
}

fn column(id: u32) -> BoundColumnReference {
    BoundColumnReference {
        table_id: TableId::new(1),
        instance_id: TableInstanceId(0),
        column_id: ColumnId::new(id),
    }
}

fn text(value: &str) -> Value {
    Value::Varchar(value.to_owned())
}

fn row(slot: u16, amount: Value, category: Value) -> QueryRow {
    QueryRow::new(vec![TableRow {
        row_id: RowId::new(PageId::new(u64::from(slot)), SlotId::new(0)),
        instance_id: TableInstanceId(0),
        values: vec![amount, category],
    }])
}

fn projections() -> Vec<BoundProjection> {
    vec![
        BoundProjection::Column(column(2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
        BoundProjection::Aggregate(BoundAggregate::Sum(column(1))),
    ]
}

fn results(state: &GroupState) -> Vec<(Vec<Value>, Vec<Value>)> {
    state
        .groups
        .iter()
        .map(|group| {
            let values = group
                .aggregates
                .iter()
                .map(|aggregate| aggregate.result().expect("집계 결과를 읽어야 함"))
                .collect();
            (group.key.clone(), values)
        })
        .collect()
}

#[test]
fn group_state는_첫_행과_다른_페이지의_같은_키를_누적한다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    let group_by = [column(2)];
    let projections = projections();

    state
        .push_row(
            &executor,
            &row(1, Value::BigInt(10), text("A")),
            &group_by,
            &projections,
        )
        .expect("첫 행부터 누적해야 함");
    assert_eq!(
        results(&state),
        vec![(vec![text("A")], vec![Value::BigInt(1), Value::BigInt(10)])]
    );

    for (page, amount, category) in [(2, 3, "B"), (3, 5, "A")] {
        state
            .push_row(
                &executor,
                &row(page, Value::BigInt(amount), text(category)),
                &group_by,
                &projections,
            )
            .expect("페이지와 입력 순서가 달라도 같은 그룹에 누적해야 함");
    }
    assert_eq!(
        results(&state),
        vec![
            (vec![text("A")], vec![Value::BigInt(2), Value::BigInt(15)]),
            (vec![text("B")], vec![Value::BigInt(1), Value::BigInt(3)]),
        ]
    );
}

#[test]
fn group_state는_column을_건너뛰고_집계_projection_순서를_유지한다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    let projections = [
        BoundProjection::Column(column(2)),
        BoundProjection::Aggregate(BoundAggregate::Sum(column(1))),
        BoundProjection::Column(column(2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
        BoundProjection::Aggregate(BoundAggregate::Sum(column(1))),
    ];

    for (page, amount) in [(1, 10), (2, 5)] {
        state
            .push_row(
                &executor,
                &row(page, Value::BigInt(amount), text("A")),
                &[column(2)],
                &projections,
            )
            .expect("집계 항목마다 별도 상태를 갱신해야 함");
    }
    assert_eq!(
        results(&state),
        vec![(
            vec![text("A")],
            vec![Value::BigInt(15), Value::BigInt(2), Value::BigInt(15)],
        )]
    );
}

#[test]
fn group_state는_null_키를_묶고_sum의_null을_건너뛴다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    for (page, amount, category) in [
        (1, Value::Null, Value::Null),
        (2, Value::Null, text("A")),
        (3, Value::Int(7), text("A")),
        (4, Value::Null, Value::Null),
        (5, Value::BigInt(5), text("A")),
    ] {
        state
            .push_row(
                &executor,
                &row(page, amount, category),
                &[column(2)],
                &projections(),
            )
            .expect("NULL과 정수 입력을 누적해야 함");
    }
    assert_eq!(
        results(&state),
        vec![
            (vec![Value::Null], vec![Value::BigInt(2), Value::Null]),
            (vec![text("A")], vec![Value::BigInt(3), Value::BigInt(12)]),
        ]
    );
}

#[test]
fn group_state는_복합_키의_모든_컬럼과_null을_구분한다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    let projections = [BoundProjection::Aggregate(BoundAggregate::CountAll)];
    for (page, amount, category) in [
        (1, Value::BigInt(1), text("A")),
        (2, Value::BigInt(1), text("B")),
        (3, Value::BigInt(2), text("A")),
        (4, Value::BigInt(1), text("A")),
        (5, Value::Null, text("A")),
        (6, Value::Null, text("A")),
        (7, Value::Null, Value::Null),
    ] {
        state
            .push_row(
                &executor,
                &row(page, amount, category),
                &[column(1), column(2)],
                &projections,
            )
            .expect("복합 키를 기준으로 그룹을 구분해야 함");
    }
    assert_eq!(
        results(&state),
        vec![
            (vec![Value::BigInt(1), text("A")], vec![Value::BigInt(2)]),
            (vec![Value::BigInt(1), text("B")], vec![Value::BigInt(1)]),
            (vec![Value::BigInt(2), text("A")], vec![Value::BigInt(1)]),
            (vec![Value::Null, text("A")], vec![Value::BigInt(2)]),
            (vec![Value::Null, Value::Null], vec![Value::BigInt(1)]),
        ]
    );
}

#[test]
fn group_state는_집계가_없는_column_projection도_그룹을_생성한다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    for (page, category) in [(1, "B"), (2, "A"), (3, "B")] {
        state
            .push_row(
                &executor,
                &row(page, Value::BigInt(1), text(category)),
                &[column(2)],
                &[BoundProjection::Column(column(2))],
            )
            .expect("집계가 없어도 중복 키를 같은 그룹으로 묶어야 함");
    }
    assert_eq!(
        results(&state),
        vec![(vec![text("B")], vec![]), (vec![text("A")], vec![])]
    );
}

#[test]
fn group_state는_입력이_없으면_그룹이_없다() {
    assert!(
        GroupState::new()
            .into_rows(&[column(2)], &projections())
            .expect("빈 입력은 결과 행을 생성하면 안 됨")
            .is_empty()
    );
}

#[test]
fn into_rows는_그룹_키와_집계를_projection_순서대로_반환한다() {
    let database = database();
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    let group_by = [column(1), column(2)];
    let projections = [
        BoundProjection::Aggregate(BoundAggregate::Sum(column(1))),
        BoundProjection::Column(column(2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
        BoundProjection::Column(column(1)),
    ];
    for (page, amount, category) in [(1, 10, "A"), (2, 3, "B"), (3, 10, "A")] {
        state
            .push_row(
                &executor,
                &row(page, Value::BigInt(amount), text(category)),
                &group_by,
                &projections,
            )
            .expect("복합 키에 집계를 누적해야 함");
    }
    assert_eq!(
        state
            .into_rows(&group_by, &projections)
            .expect("projection 순서대로 결과를 확정해야 함"),
        vec![
            vec![
                Value::BigInt(20),
                text("A"),
                Value::BigInt(2),
                Value::BigInt(10)
            ],
            vec![
                Value::BigInt(3),
                text("B"),
                Value::BigInt(1),
                Value::BigInt(3)
            ],
        ]
    );
}

#[test]
fn group_state는_같은_그룹의_sum_overflow만_오류로_반환한다() {
    let database = database();
    let executor = Executor::new(&database);
    let projections = [BoundProjection::Aggregate(BoundAggregate::Sum(column(1)))];
    for (first, addend) in [(i64::MAX, 1), (i64::MIN, -1)] {
        let mut state = GroupState::new();
        for (page, amount, category) in [(1, first, "A"), (2, addend, "B")] {
            state
                .push_row(
                    &executor,
                    &row(page, Value::BigInt(amount), text(category)),
                    &[column(2)],
                    &projections,
                )
                .expect("서로 다른 그룹의 SUM은 독립적이어야 함");
        }
        assert_eq!(
            results(&state),
            vec![
                (vec![text("A")], vec![Value::BigInt(first)]),
                (vec![text("B")], vec![Value::BigInt(addend)]),
            ]
        );
        assert!(matches!(
            state.push_row(
                &executor,
                &row(3, Value::BigInt(addend), text("A")),
                &[column(2)],
                &projections,
            ),
            Err(ExecutorError::SumOverflow)
        ));
    }
}

#[test]
fn group_state는_키와_sum의_컬럼_조회_오류를_전파한다() {
    let database = database();
    let executor = Executor::new(&database);
    let input = row(1, Value::BigInt(10), text("A"));
    for (group_by, projections) in [
        (vec![column(99)], projections()),
        (
            vec![column(2)],
            vec![BoundProjection::Aggregate(BoundAggregate::Sum(column(99)))],
        ),
    ] {
        assert!(matches!(
            GroupState::new().push_row(&executor, &input, &group_by, &projections),
            Err(ExecutorError::ColumnNotFound(id)) if id == ColumnId::new(99)
        ));
    }
}

#[test]
fn group_state는_all_projection을_거부한다() {
    let database = database();
    let executor = Executor::new(&database);
    assert!(matches!(
        GroupState::new().push_row(
            &executor,
            &row(1, Value::BigInt(10), text("A")),
            &[column(2)],
            &[BoundProjection::All],
        ),
        Err(ExecutorError::Unsupported)
    ));
}
