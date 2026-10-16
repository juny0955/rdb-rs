use crate::{
    catalog::metadata::TableId,
    executor::group::GroupState,
    query::{
        binder::{BoundAggregate, BoundOrderBy, BoundProjection},
        common::SortDirection,
    },
    tuple::Value,
};

use super::{Executor, collect_select, database, row, rows, select, users_column};

#[test]
fn group_state는_동일_key와_null을_각각_그룹으로_만든다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut state = GroupState::new();
    let group_by = [users_column(table_id, 2)];
    let projections = [
        BoundProjection::Column(users_column(table_id, 2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
    ];
    for input in [
        row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned())),
        row(2, Value::BigInt(2), Value::Varchar("Lee".to_owned())),
        row(3, Value::BigInt(3), Value::Varchar("Kim".to_owned())),
        row(4, Value::BigInt(4), Value::Null),
    ] {
        state
            .push_row(&executor, &input, &group_by, &projections)
            .expect("같은 키를 같은 그룹으로 누적해야 함");
    }
    assert_eq!(
        state
            .into_rows(&group_by, &projections, None)
            .expect("그룹 결과를 반환해야 함"),
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(2)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
            vec![Value::Null, Value::BigInt(1)],
        ]
    );
}

#[test]
fn group_state는_group_key_count_sum을_projection_순서로_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let group_by = [users_column(table_id, 2)];
    let projections = [
        BoundProjection::Column(users_column(table_id, 2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
        BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
    ];
    let mut state = GroupState::new();
    for input in [
        row(1, Value::BigInt(10), Value::Varchar("Kim".to_owned())),
        row(2, Value::BigInt(3), Value::Varchar("Lee".to_owned())),
        row(3, Value::Null, Value::Varchar("Kim".to_owned())),
    ] {
        state
            .push_row(&executor, &input, &group_by, &projections)
            .expect("그룹에 COUNT와 SUM을 누적해야 함");
    }

    assert_eq!(
        state
            .into_rows(&group_by, &projections, None)
            .expect("aggregate가 성공해야 함"),
        vec![
            vec![
                Value::Varchar("Kim".to_owned()),
                Value::BigInt(2),
                Value::BigInt(10),
            ],
            vec![
                Value::Varchar("Lee".to_owned()),
                Value::BigInt(1),
                Value::BigInt(3),
            ],
        ]
    );
}

#[test]
fn select_aggregate는_count_sum과_null을처리한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );

    assert_eq!(
        collect_select(&Executor::new(&database), rows(), bound)
            .expect("aggregate SELECT가 성공해야 함"),
        vec![vec![Value::BigInt(3), Value::BigInt(3)]]
    );
}

#[test]
fn select_aggregate는_빈_입력에서_count_0과_sum_null을_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );

    assert_eq!(
        collect_select(&Executor::new(&database), vec![], bound)
            .expect("빈 입력의 aggregate SELECT가 성공해야 함"),
        vec![vec![Value::BigInt(0), Value::Null]]
    );
}

#[test]
fn select_aggregate는_null_두_행에서_count_2와_sum_null을_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );

    assert_eq!(
        collect_select(
            &Executor::new(&database),
            vec![
                row(1, Value::Null, Value::Null),
                row(2, Value::Null, Value::Null),
            ],
            bound,
        )
        .expect("NULL 입력의 aggregate SELECT가 성공해야 함"),
        vec![vec![Value::BigInt(2), Value::Null]]
    );
}

#[test]
fn select_group_by는_order_by와_limit을적용한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(
        table_id,
        vec![
            BoundProjection::Column(users_column(table_id, 2)),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
        ],
    );
    bound.group_by = Some(vec![users_column(table_id, 2)]);
    bound.order_by = Some(BoundOrderBy {
        column: users_column(table_id, 2),
        direction: SortDirection::Desc,
    });
    bound.limit = Some(2);

    assert_eq!(
        collect_select(&Executor::new(&database), rows(), bound)
            .expect("GROUP BY SELECT가 성공해야 함"),
        vec![
            vec![Value::Varchar("Null".to_owned()), Value::BigInt(1)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
        ]
    );
}

#[test]
fn sum은_overflow를_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = select(
        table_id,
        vec![BoundProjection::Aggregate(BoundAggregate::Sum(
            users_column(table_id, 1),
        ))],
    );

    assert!(matches!(
        collect_select(
            &Executor::new(&database),
            vec![
                row(1, Value::BigInt(i64::MAX), Value::Varchar("A".to_owned())),
                row(2, Value::BigInt(1), Value::Varchar("B".to_owned())),
            ],
            bound,
        ),
        Err(super::super::ExecutorError::SumOverflow)
    ));
}

#[test]
fn aggregate_limit_zero는_빈_결과를_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Aggregate(BoundAggregate::CountAll)],
    );
    bound.limit = Some(0);

    assert!(
        collect_select(&Executor::new(&database), rows(), bound)
            .expect("aggregate LIMIT이 성공해야 함")
            .is_empty()
    );
}
