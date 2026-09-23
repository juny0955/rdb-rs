use crate::{
    catalog::metadata::TableId,
    query::{
        binder::{BoundAggregate, BoundOrderBy, BoundProjection},
        common::SortDirection,
    },
    tuple::Value,
};

use super::{Executor, database, row, rows, select, users_column};

#[test]
fn group_rows는_동일_key와_null을_각각_그룹으로_만든다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let grouped = Executor::new(&database)
        .group_rows(
            vec![
                row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned())),
                row(2, Value::BigInt(2), Value::Varchar("Lee".to_owned())),
                row(3, Value::BigInt(3), Value::Varchar("Kim".to_owned())),
                row(4, Value::BigInt(4), Value::Null),
            ],
            &[users_column(table_id, 2)],
        )
        .expect("GROUP BY가 성공해야 함");

    assert_eq!(grouped.len(), 3);
    assert_eq!(grouped[0].key, vec![Value::Varchar("Kim".to_owned())]);
    assert_eq!(grouped[0].rows.len(), 2);
    assert_eq!(grouped[2].key, vec![Value::Null]);
}

#[test]
fn aggregate_groups는_group_key_count_sum을_projection_순서로_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let group_by = [users_column(table_id, 2)];
    let groups = executor
        .group_rows(
            vec![
                row(1, Value::BigInt(10), Value::Varchar("Kim".to_owned())),
                row(2, Value::BigInt(3), Value::Varchar("Lee".to_owned())),
                row(3, Value::Null, Value::Varchar("Kim".to_owned())),
            ],
            &group_by,
        )
        .expect("GROUP BY가 성공해야 함");

    assert_eq!(
        executor
            .aggregate_groups(
                groups,
                &group_by,
                &[
                    BoundProjection::Column(users_column(table_id, 2)),
                    BoundProjection::Aggregate(BoundAggregate::CountAll),
                    BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
                ],
            )
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
        Executor::new(&database)
            .select_rows(rows(), &bound)
            .expect("aggregate SELECT가 성공해야 함"),
        vec![vec![Value::BigInt(3), Value::BigInt(3)]]
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
        Executor::new(&database)
            .select_rows(rows(), &bound)
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
        Executor::new(&database).select_rows(
            vec![
                row(1, Value::BigInt(i64::MAX), Value::Varchar("A".to_owned())),
                row(2, Value::BigInt(1), Value::Varchar("B".to_owned())),
            ],
            &bound,
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
        Executor::new(&database)
            .select_rows(rows(), &bound)
            .expect("aggregate LIMIT이 성공해야 함")
            .is_empty()
    );
}
