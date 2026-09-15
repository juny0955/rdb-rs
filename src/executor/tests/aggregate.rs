use crate::{
    catalog::metadata::{ColumnId, TableId},
    executor::Executor,
    query::binder::{BoundAggregate, BoundProjection, BoundSelect},
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
        order_by: None,
        limit,
    }
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
