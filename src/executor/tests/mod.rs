use std::{io::ErrorKind, path::Path};

use crate::{
    binder::{
        BoundAssignment, BoundDelete, BoundExpression, BoundInsert, BoundProjection, BoundSelect,
        BoundUpdate,
    },
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, RelationId, TableId, TableMetadata,
    },
    storage::page::{PageError, PageId, RowId, SlotId},
    storage::{StorageError, StorageManager, file::RelationFileManagerError},
    table::{TableError, heap::HeapTable},
    test_supports::TestDirectory,
    tuple::{Value, decode, encode},
};

use super::{Executor, ExecutorError};

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

fn storage_manager(table_id: TableId, path: &Path) -> StorageManager {
    let mut storage_manager = StorageManager::with_capacity(
        path.parent()
            .expect("table path의 부모 디렉터리가 있어야 함"),
        16,
    );
    storage_manager
        .create_relation(RelationId::Heap(table_id))
        .expect("테이블 relation을 생성해야 함");
    storage_manager
}

fn select_all(table_id: TableId) -> BoundSelect {
    BoundSelect {
        table_id,
        projections: vec![BoundProjection::All],
        filter: None,
    }
}

fn select_name(table_id: TableId) -> BoundSelect {
    BoundSelect {
        table_id,
        projections: vec![BoundProjection::Column(ColumnId::new(2))],
        filter: None,
    }
}

fn select_name_then_all(table_id: TableId) -> BoundSelect {
    BoundSelect {
        table_id,
        projections: vec![
            BoundProjection::Column(ColumnId::new(2)),
            BoundProjection::All,
        ],
        filter: None,
    }
}

fn select_name_equals(table_id: TableId, value: Value) -> BoundSelect {
    BoundSelect {
        table_id,
        projections: vec![BoundProjection::All],
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(2),
            value,
        }),
    }
}

#[test]
fn insert는_리터럴을_row로_변환해_테이블에_저장한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("insert");
    let path = directory.path().join("1.tbl");
    let bound = BoundInsert {
        table_id,
        values: vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
    };

    HeapTable::new(table_id);
    let mut storage_manager = storage_manager(table_id, &path);
    let row = Executor::new(&database)
        .encode_insert(&bound)
        .expect("INSERT 값을 Row로 변환해야 함");
    let row_id = {
        let mut heap_table = HeapTable::new(table_id);
        heap_table
            .insert(&row, &mut storage_manager)
            .expect("Row를 저장해야 함")
    };

    let mut table = HeapTable::new(table_id);
    let row = table
        .get(&mut storage_manager, row_id)
        .expect("삽입한 Row를 읽어야 함");
    let values = decode(&row, &columns).expect("Row를 값으로 변환해야 함");

    assert_eq!(
        values,
        vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())]
    );
}

#[test]
fn update는_필터와_일치하는_row만_수정하고_재시작후에도_유지한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("update-filter");
    let path = directory.path().join("1.tbl");
    let kim = encode(
        &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        &columns,
    )
    .expect("Kim Row를 변환해야 함");
    let lee = encode(
        &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
        &columns,
    )
    .expect("Lee Row를 변환해야 함");

    let mut storage_manager = storage_manager(table_id, &path);
    let (kim_id, lee_id) = {
        let mut table = HeapTable::new(table_id);
        let kim_id = table
            .insert(&kim, &mut storage_manager)
            .expect("Kim Row를 삽입해야 함");
        let lee_id = table
            .insert(&lee, &mut storage_manager)
            .expect("Lee Row를 삽입해야 함");
        (kim_id, lee_id)
    };

    let bound = BoundUpdate {
        table_id,
        assignments: vec![BoundAssignment {
            column_id: ColumnId::new(2),
            value: Value::Varchar("Park".to_owned()),
        }],
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(1),
            value: Value::BigInt(1),
        }),
    };

    let updated = {
        let mut heap_table = HeapTable::new(table_id);
        let executor = Executor::new(&database);
        let rows = heap_table
            .scan(&mut storage_manager)
            .expect("테이블을 scan해야 함");
        let updated = executor
            .prepare_update(&bound, rows)
            .expect("UPDATE 대상을 계산해야 함");

        for result in &updated {
            heap_table
                .update(&mut storage_manager, result.row_id, &result.new_row)
                .expect("UPDATE 대상을 저장소에 반영해야 함");
        }

        updated
    };

    assert_eq!(updated.len(), 1);

    let mut table = HeapTable::new(table_id);
    let kim = decode(
        &table
            .get(&mut storage_manager, kim_id)
            .expect("수정한 Kim Row를 읽어야 함"),
        &columns,
    )
    .expect("Kim Row를 값으로 변환해야 함");
    let lee = decode(
        &table
            .get(&mut storage_manager, lee_id)
            .expect("유지된 Lee Row를 읽어야 함"),
        &columns,
    )
    .expect("Lee Row를 값으로 변환해야 함");

    assert_eq!(
        kim,
        vec![Value::BigInt(1), Value::Varchar("Park".to_owned())]
    );
    assert_eq!(
        lee,
        vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())]
    );
}

#[test]
fn delete는_필터와_일치하는_row만_삭제하고_재시작후에도_유지한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("delete-filter");
    let path = directory.path().join("1.tbl");
    let kim = encode(
        &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        &columns,
    )
    .expect("Kim Row를 변환해야 함");
    let lee = encode(
        &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
        &columns,
    )
    .expect("Lee Row를 변환해야 함");

    let mut storage_manager = storage_manager(table_id, &path);
    let (kim_id, lee_id) = {
        let mut table = HeapTable::new(table_id);
        let kim_id = table
            .insert(&kim, &mut storage_manager)
            .expect("Kim Row를 삽입해야 함");
        let lee_id = table
            .insert(&lee, &mut storage_manager)
            .expect("Lee Row를 삽입해야 함");
        (kim_id, lee_id)
    };

    let bound = BoundDelete {
        table_id,
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(1),
            value: Value::BigInt(1),
        }),
    };

    let deleted = {
        let mut heap_table = HeapTable::new(table_id);
        let executor = Executor::new(&database);
        let rows = heap_table
            .scan(&mut storage_manager)
            .expect("테이블을 scan해야 함");
        let deleted = executor
            .prepare_delete(&bound, rows)
            .expect("DELETE 대상을 계산해야 함");

        for result in &deleted {
            heap_table
                .delete(&mut storage_manager, result.row_id)
                .expect("DELETE 대상을 저장소에서 삭제해야 함");
        }

        deleted
    };

    assert_eq!(deleted.len(), 1);

    let mut table = HeapTable::new(table_id);
    assert!(matches!(
        table.get(&mut storage_manager, kim_id),
        Err(TableError::Page(PageError::SlotNotFound))
    ));

    let lee = decode(
        &table
            .get(&mut storage_manager, lee_id)
            .expect("삭제하지 않은 Lee Row를 읽어야 함"),
        &columns,
    )
    .expect("Lee Row를 값으로 변환해야 함");
    assert_eq!(
        lee,
        vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())]
    );
}

#[test]
fn select는_테이블의_모든_row를_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("select-all");
    let path = directory.path().join("1.tbl");
    let first_values = vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())];
    let second_values = vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())];
    let first_row = encode(&first_values, &columns).expect("첫 Row를 변환해야 함");
    let second_row = encode(&second_values, &columns).expect("둘째 Row를 변환해야 함");
    let mut storage_manager = storage_manager(table_id, &path);

    {
        let mut table = HeapTable::new(table_id);
        table
            .insert(&first_row, &mut storage_manager)
            .expect("첫 Row를 삽입해야 함");
        table
            .insert(&second_row, &mut storage_manager)
            .expect("둘째 Row를 삽입해야 함");
    }

    let mut heap_table = HeapTable::new(table_id);
    let executor = Executor::new(&database);
    let scanned_rows = heap_table
        .scan(&mut storage_manager)
        .expect("테이블을 scan해야 함");
    let rows = executor
        .projection_and_filtered_rows(scanned_rows, &select_all(table_id))
        .expect("SELECT가 성공해야 함");

    assert_eq!(rows, vec![first_values, second_values]);
}

#[test]
fn select는_지정한_컬럼만_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("select-name");
    let path = directory.path().join("1.tbl");
    let kim = encode(
        &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        &columns,
    )
    .expect("Kim Row를 변환해야 함");
    let lee = encode(
        &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
        &columns,
    )
    .expect("Lee Row를 변환해야 함");
    let mut storage_manager = storage_manager(table_id, &path);

    {
        let mut table = HeapTable::new(table_id);
        table
            .insert(&kim, &mut storage_manager)
            .expect("Kim Row를 삽입해야 함");
        table
            .insert(&lee, &mut storage_manager)
            .expect("Lee Row를 삽입해야 함");
    }

    let mut heap_table = HeapTable::new(table_id);
    let executor = Executor::new(&database);
    let scanned_rows = heap_table
        .scan(&mut storage_manager)
        .expect("테이블을 scan해야 함");
    let rows = executor
        .projection_and_filtered_rows(scanned_rows, &select_name(table_id))
        .expect("SELECT가 성공해야 함");

    assert_eq!(
        rows,
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
}

#[test]
fn select는_projection_목록_순서대로_값을_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("select-name-then-all");
    let path = directory.path().join("1.tbl");
    let row = encode(
        &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        &columns,
    )
    .expect("Row를 변환해야 함");
    let mut storage_manager = storage_manager(table_id, &path);

    {
        let mut table = HeapTable::new(table_id);
        table
            .insert(&row, &mut storage_manager)
            .expect("Row를 삽입해야 함");
    }

    let mut heap_table = HeapTable::new(table_id);
    let executor = Executor::new(&database);
    let scanned_rows = heap_table
        .scan(&mut storage_manager)
        .expect("테이블을 scan해야 함");
    let rows = executor
        .projection_and_filtered_rows(scanned_rows, &select_name_then_all(table_id))
        .expect("SELECT가 성공해야 함");

    assert_eq!(
        rows,
        vec![vec![
            Value::Varchar("Kim".to_owned()),
            Value::BigInt(1),
            Value::Varchar("Kim".to_owned()),
        ]]
    );
}

#[test]
fn select는_equal_filter와_일치하는_row만_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("select-filter");
    let path = directory.path().join("1.tbl");
    let kim_values = vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())];
    let lee_values = vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())];
    let kim = encode(&kim_values, &columns).expect("Kim Row를 변환해야 함");
    let lee = encode(&lee_values, &columns).expect("Lee Row를 변환해야 함");
    let mut storage_manager = storage_manager(table_id, &path);

    {
        let mut table = HeapTable::new(table_id);
        table
            .insert(&kim, &mut storage_manager)
            .expect("Kim Row를 삽입해야 함");
        table
            .insert(&lee, &mut storage_manager)
            .expect("Lee Row를 삽입해야 함");
    }

    let mut heap_table = HeapTable::new(table_id);
    let executor = Executor::new(&database);
    let scanned_rows = heap_table
        .scan(&mut storage_manager)
        .expect("테이블을 scan해야 함");
    let rows = executor
        .projection_and_filtered_rows(
            scanned_rows,
            &select_name_equals(table_id, Value::Varchar("Kim".to_owned())),
        )
        .expect("SELECT가 성공해야 함");

    assert_eq!(rows, vec![kim_values]);
}

#[test]
fn 전달된_후보_row에_filter와_projection을적용한다() {
    // Given
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let kim = encode(
        &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
        &columns,
    )
    .expect("Kim Row를 변환해야 함");
    let lee = encode(
        &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
        &columns,
    )
    .expect("Lee Row를 변환해야 함");
    let bound = BoundSelect {
        table_id,
        projections: vec![BoundProjection::Column(ColumnId::new(2))],
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(1),
            value: Value::BigInt(1),
        }),
    };
    let executor = Executor::new(&database);

    // When
    let rows = executor
        .projection_and_filtered_rows(
            vec![
                (RowId::new(PageId::new(1), SlotId::new(1)), kim),
                (RowId::new(PageId::new(1), SlotId::new(2)), lee),
            ],
            &bound,
        )
        .expect("후보 Row를 처리해야 함");

    // Then
    assert_eq!(rows, vec![vec![Value::Varchar("Kim".to_owned())]]);
}

#[test]
fn select에서_null_equal_filter는_row를_반환하지_않는다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let columns = users_columns();
    let directory = TestDirectory::new("select-null-filter");
    let path = directory.path().join("1.tbl");
    let null_name =
        encode(&[Value::BigInt(1), Value::Null], &columns).expect("NULL Row를 변환해야 함");
    let mut storage_manager = storage_manager(table_id, &path);

    {
        let mut table = HeapTable::new(table_id);
        table
            .insert(&null_name, &mut storage_manager)
            .expect("NULL Row를 삽입해야 함");
    }

    let mut heap_table = HeapTable::new(table_id);
    let executor = Executor::new(&database);
    let scanned_rows = heap_table
        .scan(&mut storage_manager)
        .expect("테이블을 scan해야 함");
    let rows = executor
        .projection_and_filtered_rows(scanned_rows, &select_name_equals(table_id, Value::Null))
        .expect("SELECT가 성공해야 함");

    assert!(rows.is_empty());
}

#[test]
fn select는_메타데이터에_없는_테이블을_거부한다() {
    let table_id = TableId::new(1);
    let database = DatabaseMetadata::new("test".to_owned(), vec![], vec![])
        .expect("빈 데이터베이스 메타데이터가 유효해야 함");
    let executor = Executor::new(&database);

    let result = executor.projection_and_filtered_rows(vec![], &select_all(table_id));

    assert!(matches!(result, Err(ExecutorError::TableNotFound(id)) if id == table_id));
}

#[test]
fn 없는_테이블_파일을_열어도_파일을_생성하지_않는다() {
    let table_id = TableId::new(1);
    let directory = TestDirectory::new("missing-file");
    let mut storage_manager = StorageManager::with_capacity(directory.path(), 1);
    let result = storage_manager.register_relation(RelationId::Heap(table_id));

    assert!(matches!(
        result,
        Err(StorageError::RelationFileManager(RelationFileManagerError::Io(error)))
            if error.kind() == ErrorKind::NotFound
    ));
    assert!(!directory.path().join("1.tbl").exists());
}
