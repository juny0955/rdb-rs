use crate::{
    binder::BoundStatement,
    database::{Database, DatabaseError, ExecuteResult},
    executor::Executor,
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{bind_sql, parse_sql};

#[test]
fn 두번째_select는_cache된_page를_사용한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-select-cache-hit");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;
    }

    let mut database = Database::open(directory.path(), "reopened")?;
    let select = parse_sql("SELECT * FROM users;");
    let expected = vec![vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())]];

    assert!(matches!(
        database.execute(&select)?,
        ExecuteResult::Rows(rows) if rows == expected
    ));

    std::fs::rename(
        directory.path().join("1.tbl"),
        directory.path().join("cached-1.tbl"),
    )
    .expect("첫 SELECT 후 table file 이름을 변경해야 함");

    assert!(matches!(
        database.execute(&select)?,
        ExecuteResult::Rows(rows) if rows == expected
    ));
    Ok(())
}

#[test]
fn sql_crud는_parser부터_file까지_동작한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("sql-crud-integration");
    let mut database = Database::open(directory.path(), "test")?;

    let BoundStatement::CreateTable(bound) = bind_sql(
        "CREATE TABLE users (id BIGINT, name VARCHAR);",
        &database.metadata,
    ) else {
        panic!("CREATE TABLE이 bind되어야 함");
    };
    database.create_table(&bound)?;

    let BoundStatement::Insert(bound) =
        bind_sql("INSERT INTO users VALUES (1, 'Kim');", &database.metadata)
    else {
        panic!("INSERT가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    Executor::new(&database.metadata)
        .execute_insert(&bound, heap_table, &mut database.buffer_pool)
        .expect("INSERT가 실행되어야 함");

    let BoundStatement::Select(bound) =
        bind_sql("SELECT name FROM users WHERE id = 1;", &database.metadata)
    else {
        panic!("SELECT가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    let rows = Executor::new(&database.metadata)
        .execute_select(&bound, heap_table, &mut database.buffer_pool)
        .expect("SELECT가 실행되어야 함");
    assert_eq!(rows, vec![vec![Value::Varchar("Kim".to_owned())]]);

    let BoundStatement::Update(bound) = bind_sql(
        "UPDATE users SET name = 'Lee' WHERE id = 1;",
        &database.metadata,
    ) else {
        panic!("UPDATE가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    let updated = Executor::new(&database.metadata)
        .execute_update(&bound, heap_table, &mut database.buffer_pool)
        .expect("UPDATE가 실행되어야 함");
    assert_eq!(updated.len(), 1);

    let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata) else {
        panic!("SELECT가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    let rows = Executor::new(&database.metadata)
        .execute_select(&bound, heap_table, &mut database.buffer_pool)
        .expect("SELECT가 실행되어야 함");
    assert_eq!(
        rows,
        vec![vec![Value::BigInt(1), Value::Varchar("Lee".to_owned())]]
    );

    let BoundStatement::Delete(bound) =
        bind_sql("DELETE FROM users WHERE id = 1;", &database.metadata)
    else {
        panic!("DELETE가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    let deleted = Executor::new(&database.metadata)
        .execute_delete(&bound, heap_table, &mut database.buffer_pool)
        .expect("DELETE가 실행되어야 함");
    assert_eq!(deleted.len(), 1);

    let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata) else {
        panic!("SELECT가 bind되어야 함");
    };
    let heap_table = database.table_cache.get_or_open_table(
        &mut database.buffer_pool,
        database.data_dir.as_path(),
        bound.table_id,
    )?;
    let rows = Executor::new(&database.metadata)
        .execute_select(&bound, heap_table, &mut database.buffer_pool)
        .expect("SELECT가 실행되어야 함");
    assert!(rows.is_empty());
    Ok(())
}
