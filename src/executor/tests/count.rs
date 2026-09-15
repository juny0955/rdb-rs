use crate::{
    binder::{BoundProjection, BoundSelect},
    catalog::metadata::TableId,
    executor::Executor,
    storage::page::{PageId, RowId, SlotId},
    tuple::{Value, encode},
};

use super::{database, users_columns};

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
        projections: vec![BoundProjection::CountAll],
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
