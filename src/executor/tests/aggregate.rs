use crate::{
    catalog::metadata::{ColumnId, TableId},
    executor::{
        Executor,
        projection::{aggregate_groups, group_rows},
    },
    query::{
        binder::{BoundAggregate, BoundOrderBy, BoundProjection, BoundSelect},
        common::SortDirection,
    },
    storage::page::{PageId, RowId, SlotId},
    tuple::{Value, encode},
};

use super::{ExecutorError, database, users_columns};

fn sum_bound(table_id: TableId, limit: Option<usize>) -> BoundSelect {
    BoundSelect {
        table_id,
        projections: vec![BoundProjection::Aggregate(BoundAggregate::Sum(
            ColumnId::new(1),
        ))],
        filter: None,
        group_by: None,
        order_by: None,
        limit,
    }
}

#[test]
fn group_rows는_동일_key와_null을_각각_그룹으로_만든다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let table = database
        .table_by_id(table_id)
        .expect("users 테이블이 존재해야 함");
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("두 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(4)),
            encode(&[Value::BigInt(4), Value::Null], &columns).expect("NULL row를 변환해야 함"),
        ),
    ];

    // When
    let groups = group_rows(rows, table, &[ColumnId::new(2)]).expect("grouping이 성공해야 함");

    // Then
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].key, vec![Value::Varchar("Kim".to_owned())]);
    assert_eq!(groups[0].rows.len(), 2);
    assert_eq!(groups[1].key, vec![Value::Varchar("Lee".to_owned())]);
    assert_eq!(groups[1].rows.len(), 1);
    assert_eq!(groups[2].key, vec![Value::Null]);
    assert_eq!(groups[2].rows.len(), 1);
}

#[test]
fn aggregate_groups는_각_그룹의_row_수를_반환한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let table = database
        .table_by_id(table_id)
        .expect("users 테이블이 존재해야 함");
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("두 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(4)),
            encode(&[Value::BigInt(4), Value::Null], &columns).expect("NULL row를 변환해야 함"),
        ),
    ];
    let groups = group_rows(rows, table, &[ColumnId::new(2)]).expect("grouping이 성공해야 함");

    // When
    let group_by = [ColumnId::new(2)];
    let projections = [BoundProjection::Aggregate(BoundAggregate::CountAll)];
    let counts = aggregate_groups(groups, &group_by, table, &projections, None)
        .expect("group count가 성공해야 함");

    // Then
    assert_eq!(
        counts,
        vec![
            vec![Value::BigInt(2)],
            vec![Value::BigInt(1)],
            vec![Value::BigInt(1)],
        ]
    );
}

#[test]
fn aggregate_groups는_group_by_컬럼과_count를_projection_순서로_반환한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let table = database
        .table_by_id(table_id)
        .expect("users 테이블이 존재해야 함");
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("두 번째 Kim row를 변환해야 함"),
        ),
    ];
    let group_by = [ColumnId::new(2)];
    let projections = [
        BoundProjection::Column(ColumnId::new(2)),
        BoundProjection::Aggregate(BoundAggregate::CountAll),
    ];
    let groups = group_rows(rows, table, &group_by).expect("grouping이 성공해야 함");

    // When
    let result = aggregate_groups(groups, &group_by, table, &projections, None)
        .expect("group count projection이 성공해야 함");

    // Then
    assert_eq!(
        result,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(2)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
        ]
    );
}

#[test]
fn select_count_all을_group_by_컬럼과_함께_실행한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("두 번째 Kim row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Column(ColumnId::new(2)),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
        ],
        filter: None,
        group_by: Some(vec![ColumnId::new(2)]),
        order_by: None,
        limit: None,
    };

    // When
    let result = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("GROUP BY COUNT(*)가 성공해야 함");

    // Then
    assert_eq!(
        result,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(2)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
        ]
    );
}

#[test]
fn select_count_all은_group_by후_order_by_asc와_limit을_적용한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(&[Value::BigInt(2), Value::Null], &columns).expect("NULL row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(4)),
            encode(
                &[Value::BigInt(4), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("두 번째 Kim row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Column(ColumnId::new(2)),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
        ],
        filter: None,
        group_by: Some(vec![ColumnId::new(2)]),
        order_by: Some(BoundOrderBy {
            column_id: ColumnId::new(2),
            direction: SortDirection::Asc,
        }),
        limit: Some(2),
    };

    // When
    let result = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("GROUP BY ORDER BY ASC가 성공해야 함");

    // Then
    assert_eq!(
        result,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(2)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
        ]
    );
}

#[test]
fn select_count_all은_group_by후_order_by_desc에서_null을_처음에_반환한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(&[Value::BigInt(2), Value::Null], &columns).expect("NULL row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Column(ColumnId::new(2)),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
        ],
        filter: None,
        group_by: Some(vec![ColumnId::new(2)]),
        order_by: Some(BoundOrderBy {
            column_id: ColumnId::new(2),
            direction: SortDirection::Desc,
        }),
        limit: None,
    };

    // When
    let result = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("GROUP BY ORDER BY DESC가 성공해야 함");

    // Then
    assert_eq!(
        result,
        vec![
            vec![Value::Null, Value::BigInt(1)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(1)],
        ]
    );
}

#[test]
fn select_sum을_group_by_컬럼과_함께_실행한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(10), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(&[Value::Null, Value::Varchar("Kim".to_owned())], &columns)
                .expect("두 번째 Kim row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Column(ColumnId::new(2)),
            BoundProjection::Aggregate(BoundAggregate::Sum(ColumnId::new(1))),
        ],
        filter: None,
        group_by: Some(vec![ColumnId::new(2)]),
        order_by: None,
        limit: None,
    };

    // When
    let result = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("GROUP BY SUM이 성공해야 함");

    // Then
    assert_eq!(
        result,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(10)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(3)],
        ]
    );
}

#[test]
fn select_count_all은_모든_row_수를_반환한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("Kim row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("Lee row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Park".to_owned())],
                &columns,
            )
            .expect("Park row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![BoundProjection::Aggregate(BoundAggregate::CountAll)],
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    };

    // When
    let rows = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("COUNT(*)가 성공해야 함");

    // Then
    assert_eq!(rows, vec![vec![Value::BigInt(3)]]);
}

#[test]
fn select_count_all과_sum을_projection_순서대로_반환한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(10), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(&[Value::Null, Value::Varchar("Lee".to_owned())], &columns)
                .expect("NULL row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(
                &[Value::BigInt(3), Value::Varchar("Park".to_owned())],
                &columns,
            )
            .expect("세 번째 row를 변환해야 함"),
        ),
    ];
    let bound = BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(ColumnId::new(1))),
        ],
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    };

    // When
    let result = Executor::new(&database)
        .select_rows(rows, &bound)
        .expect("COUNT와 SUM이 성공해야 함");

    // Then
    assert_eq!(result, vec![vec![Value::BigInt(3), Value::BigInt(13)]]);
}

#[test]
fn select_sum은_null을_제외한_값을_합산한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(10), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("첫 번째 row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(-3), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("두 번째 row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(3)),
            encode(&[Value::Null, Value::Varchar("Park".to_owned())], &columns)
                .expect("NULL row를 변환해야 함"),
        ),
    ];

    let result = Executor::new(&database)
        .select_rows(rows, &sum_bound(table_id, None))
        .expect("SUM이 성공해야 함");

    assert_eq!(result, vec![vec![Value::BigInt(7)]]);
}

#[test]
fn select_sum은_모든_값이_null이면_null을_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(&[Value::Null, Value::Varchar("Kim".to_owned())], &columns)
                .expect("첫 번째 NULL row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(&[Value::Null, Value::Varchar("Lee".to_owned())], &columns)
                .expect("두 번째 NULL row를 변환해야 함"),
        ),
    ];

    let result = Executor::new(&database)
        .select_rows(rows, &sum_bound(table_id, None))
        .expect("SUM이 성공해야 함");

    assert_eq!(result, vec![vec![Value::Null]]);
}

#[test]
fn select_sum은_overflow를_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![
        (
            RowId::new(PageId::new(1), SlotId::new(1)),
            encode(
                &[Value::BigInt(i64::MAX), Value::Varchar("Kim".to_owned())],
                &columns,
            )
            .expect("최댓값 row를 변환해야 함"),
        ),
        (
            RowId::new(PageId::new(1), SlotId::new(2)),
            encode(
                &[Value::BigInt(1), Value::Varchar("Lee".to_owned())],
                &columns,
            )
            .expect("overflow row를 변환해야 함"),
        ),
    ];

    let result = Executor::new(&database).select_rows(rows, &sum_bound(table_id, None));

    assert!(matches!(result, Err(ExecutorError::SumOverflow)));
}

#[test]
fn select_sum은_limit_zero이면_빈_결과를_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let rows = vec![(
        RowId::new(PageId::new(1), SlotId::new(1)),
        encode(
            &[Value::BigInt(10), Value::Varchar("Kim".to_owned())],
            &columns,
        )
        .expect("row를 변환해야 함"),
    )];

    let result = Executor::new(&database)
        .select_rows(rows, &sum_bound(table_id, Some(0)))
        .expect("SUM이 성공해야 함");

    assert!(result.is_empty());
}
