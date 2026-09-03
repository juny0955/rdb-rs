use std::{io::ErrorKind, path::Path};

use crate::{
    binder::{
        BoundAssignment, BoundDelete, BoundExpression, BoundInsert, BoundProjection, BoundSelect,
        BoundUpdate,
    },
    buffer::BufferPool,
    page::PageError,
    parser::ast::Literal,
    schema::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, RelationId, TableId, TableMetadata,
    },
    table::{HeapTable, HeapTableError},
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

fn buffer_pool(table_id: TableId, path: &Path) -> BufferPool {
    let mut buffer_pool = BufferPool::new(16);
    buffer_pool.register_relation(RelationId::Heap(table_id), path.to_path_buf());
    buffer_pool
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

fn select_name_equals(table_id: TableId, value: Literal) -> BoundSelect {
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
        literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
    };

    HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
    let mut buffer_pool = buffer_pool(table_id, &path);
    let row_id = {
        let mut heap_table =
            HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
        let executor = Executor::new(&database);
        executor
            .execute_insert(&bound, &mut heap_table, &mut buffer_pool)
            .expect("INSERT가 성공해야 함")
    };

    let mut table = HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let row = table
        .get(row_id, &mut buffer_pool)
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

    let mut buffer_pool = buffer_pool(table_id, &path);
    let (kim_id, lee_id) = {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        let kim_id = table
            .insert(&kim, &mut buffer_pool)
            .expect("Kim Row를 삽입해야 함");
        let lee_id = table
            .insert(&lee, &mut buffer_pool)
            .expect("Lee Row를 삽입해야 함");
        (kim_id, lee_id)
    };

    let bound = BoundUpdate {
        table_id,
        assignments: vec![BoundAssignment {
            column_id: ColumnId::new(2),
            value: Literal::String("Park".to_owned()),
        }],
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(1),
            value: Literal::Integer(1),
        }),
    };

    let updated = {
        let mut heap_table =
            HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
        let executor = Executor::new(&database);
        executor
            .execute_update(&bound, &mut heap_table, &mut buffer_pool)
            .expect("UPDATE가 성공해야 함")
    };

    assert_eq!(updated, 1);

    let mut table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 다시 열어야 함");
    let kim = decode(
        &table
            .get(kim_id, &mut buffer_pool)
            .expect("수정한 Kim Row를 읽어야 함"),
        &columns,
    )
    .expect("Kim Row를 값으로 변환해야 함");
    let lee = decode(
        &table
            .get(lee_id, &mut buffer_pool)
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

    let mut buffer_pool = buffer_pool(table_id, &path);
    let (kim_id, lee_id) = {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        let kim_id = table
            .insert(&kim, &mut buffer_pool)
            .expect("Kim Row를 삽입해야 함");
        let lee_id = table
            .insert(&lee, &mut buffer_pool)
            .expect("Lee Row를 삽입해야 함");
        (kim_id, lee_id)
    };

    let bound = BoundDelete {
        table_id,
        filter: Some(BoundExpression::Equal {
            column_id: ColumnId::new(1),
            value: Literal::Integer(1),
        }),
    };

    let deleted = {
        let mut heap_table =
            HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
        let executor = Executor::new(&database);
        executor
            .execute_delete(&bound, &mut heap_table, &mut buffer_pool)
            .expect("DELETE가 성공해야 함")
    };

    assert_eq!(deleted, 1);

    let mut table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 다시 열어야 함");
    assert!(matches!(
        table.get(kim_id, &mut buffer_pool),
        Err(HeapTableError::Page(PageError::SlotNotFound))
    ));

    let lee = decode(
        &table
            .get(lee_id, &mut buffer_pool)
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
    let mut buffer_pool = buffer_pool(table_id, &path);

    {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        table
            .insert(&first_row, &mut buffer_pool)
            .expect("첫 Row를 삽입해야 함");
        table
            .insert(&second_row, &mut buffer_pool)
            .expect("둘째 Row를 삽입해야 함");
    }

    let mut heap_table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let executor = Executor::new(&database);
    let rows = executor
        .execute_select(&select_all(table_id), &mut heap_table, &mut buffer_pool)
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
    let mut buffer_pool = buffer_pool(table_id, &path);

    {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        table
            .insert(&kim, &mut buffer_pool)
            .expect("Kim Row를 삽입해야 함");
        table
            .insert(&lee, &mut buffer_pool)
            .expect("Lee Row를 삽입해야 함");
    }

    let mut heap_table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let executor = Executor::new(&database);
    let rows = executor
        .execute_select(&select_name(table_id), &mut heap_table, &mut buffer_pool)
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
    let mut buffer_pool = buffer_pool(table_id, &path);

    {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        table
            .insert(&row, &mut buffer_pool)
            .expect("Row를 삽입해야 함");
    }

    let mut heap_table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let executor = Executor::new(&database);
    let rows = executor
        .execute_select(
            &select_name_then_all(table_id),
            &mut heap_table,
            &mut buffer_pool,
        )
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
    let mut buffer_pool = buffer_pool(table_id, &path);

    {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        table
            .insert(&kim, &mut buffer_pool)
            .expect("Kim Row를 삽입해야 함");
        table
            .insert(&lee, &mut buffer_pool)
            .expect("Lee Row를 삽입해야 함");
    }

    let mut heap_table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let executor = Executor::new(&database);
    let rows = executor
        .execute_select(
            &select_name_equals(table_id, Literal::String("Kim".to_owned())),
            &mut heap_table,
            &mut buffer_pool,
        )
        .expect("SELECT가 성공해야 함");

    assert_eq!(rows, vec![kim_values]);
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
    let mut buffer_pool = buffer_pool(table_id, &path);

    {
        let mut table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
        table
            .insert(&null_name, &mut buffer_pool)
            .expect("NULL Row를 삽입해야 함");
    }

    let mut heap_table =
        HeapTable::open_existing(table_id, &path).expect("테이블 파일을 열어야 함");
    let executor = Executor::new(&database);
    let rows = executor
        .execute_select(
            &select_name_equals(table_id, Literal::Null),
            &mut heap_table,
            &mut buffer_pool,
        )
        .expect("SELECT가 성공해야 함");

    assert!(rows.is_empty());
}

#[test]
fn select는_메타데이터에_없는_테이블을_거부한다() {
    let table_id = TableId::new(1);
    let database = DatabaseMetadata::new("test".to_owned(), vec![], vec![])
        .expect("빈 데이터베이스 메타데이터가 유효해야 함");
    let directory = TestDirectory::new("table-not-found");
    let path = directory.path().join("1.tbl");
    let mut heap_table = HeapTable::open(table_id, &path).expect("테이블 파일을 생성해야 함");
    let mut buffer_pool = buffer_pool(table_id, &path);
    let executor = Executor::new(&database);

    let result = executor.execute_select(&select_all(table_id), &mut heap_table, &mut buffer_pool);

    assert!(matches!(result, Err(ExecutorError::TableNotFound(id)) if id == table_id));
}

#[test]
fn 없는_테이블_파일을_열어도_파일을_생성하지_않는다() {
    let table_id = TableId::new(1);
    let directory = TestDirectory::new("missing-file");
    let path = directory.path().join("1.tbl");
    let result = HeapTable::open_existing(table_id, &path);

    assert!(matches!(
        result,
        Err(HeapTableError::Io(error))
            if error.kind() == ErrorKind::NotFound
    ));
    assert!(!directory.path().join("1.tbl").exists());
}
