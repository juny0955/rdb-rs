use crate::{
    catalog::metadata::{ColumnId, TableId},
    query::binder::{BoundColumnReference, BoundJoinCondition, TableInstanceId},
    storage::page::{PageId, RowId, SlotId},
    tuple::Value,
};

use super::{Executor, QueryRow, TableRow, database, row, users_column};

#[test]
fn hash_join은_중복키를_모두_결합하고_null은_제외한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let left_rows = vec![
        row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned())),
        row(2, Value::Null, Value::Varchar("Null".to_owned())),
    ];
    let right_row = |slot, id, name: &str| QueryRow {
        table_rows: vec![TableRow {
            row_id: RowId::new(PageId::new(1), SlotId::new(slot)),
            instance_id: TableInstanceId(1),
            values: vec![id, Value::Varchar(name.to_owned())],
        }],
    };
    let right_rows = vec![
        right_row(10, Value::BigInt(1), "first"),
        right_row(11, Value::BigInt(1), "second"),
        right_row(12, Value::Null, "null"),
    ];
    let condition = BoundJoinCondition {
        left: users_column(table_id, 1),
        right: BoundColumnReference {
            table_id,
            instance_id: TableInstanceId(1),
            column_id: ColumnId::new(1),
        },
    };

    // When
    let joined_rows = Executor::new(&database)
        .hash_join(&left_rows, &right_rows, &condition)
        .expect("Hash Join이 성공해야 함");

    // Then
    assert_eq!(joined_rows.len(), 2);
    assert!(joined_rows.iter().all(|joined| {
        joined.table_rows.len() == 2
            && joined.table_rows[0].instance_id == TableInstanceId(0)
            && joined.table_rows[0].values[0] == Value::BigInt(1)
            && joined.table_rows[1].instance_id == TableInstanceId(1)
            && joined.table_rows[1].values[0] == Value::BigInt(1)
    }));
    let right_names = joined_rows
        .iter()
        .map(|joined| joined.table_rows[1].values[1].clone())
        .collect::<Vec<_>>();
    assert!(right_names.contains(&Value::Varchar("first".to_owned())));
    assert!(right_names.contains(&Value::Varchar("second".to_owned())));
}
