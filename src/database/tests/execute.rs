use crate::{
    binder::BoundStatement,
    database::{Database, DatabaseError, ExecuteResult},
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
fn less_than_select는_더_작은_row만_반환한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-less-than-select");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (9, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Lee');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id < 10;"))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::Rows(rows) if rows == vec![vec![Value::Varchar("Kim".to_owned())]]
    ));
    Ok(())
}

#[test]
fn not_equal_select는_일치하지_않는_non_null_row만_반환한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-not-equal-select");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (2, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id != 1;"))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::Rows(rows) if rows == vec![vec![Value::Varchar("Lee".to_owned())]]
    ));
    Ok(())
}

#[test]
fn less_than_or_equal_select는_더_작거나_같은_non_null_row만_반환한다() -> Result<(), DatabaseError>
{
    // Given
    let directory = TestDirectory::new("database-less-than-or-equal-select");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (9, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (11, 'Park');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id <= 10;"))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::Rows(rows)
            if rows == vec![
                vec![Value::Varchar("Kim".to_owned())],
                vec![Value::Varchar("Lee".to_owned())],
            ]
    ));
    Ok(())
}

#[test]
fn greater_than_select는_더_큰_non_null_row만_반환한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-greater-than-select");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (9, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (11, 'Park');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id > 10;"))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::Rows(rows) if rows == vec![vec![Value::Varchar("Park".to_owned())]]
    ));
    Ok(())
}

#[test]
fn greater_than_or_equal_select는_더_크거나_같은_non_null_row만_반환한다()
-> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-greater-than-or-equal-select");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (9, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (11, 'Park');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id >= 10;"))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::Rows(rows)
            if rows == vec![
                vec![Value::Varchar("Lee".to_owned())],
                vec![Value::Varchar("Park".to_owned())],
            ]
    ));
    Ok(())
}

#[test]
fn sql_crud는_parser부터_file까지_동작한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("sql-crud-integration");
    let mut database = Database::open(directory.path(), "test")?;

    let BoundStatement::CreateTable(bound) = bind_sql(
        "CREATE TABLE users (id BIGINT, name VARCHAR);",
        database.catalog.metadata(),
    ) else {
        panic!("CREATE TABLE이 bind되어야 함");
    };
    database.create_table(&bound)?;

    let BoundStatement::Insert(bound) = bind_sql(
        "INSERT INTO users VALUES (1, 'Kim');",
        database.catalog.metadata(),
    ) else {
        panic!("INSERT가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_insert(&bound)?,
        ExecuteResult::Command { affected_rows: 1 }
    ));

    let BoundStatement::Select(bound) = bind_sql(
        "SELECT name FROM users WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("SELECT가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_select(&bound)?,
        ExecuteResult::Rows(rows) if rows == vec![vec![Value::Varchar("Kim".to_owned())]]
    ));

    let BoundStatement::Update(bound) = bind_sql(
        "UPDATE users SET name = 'Lee' WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("UPDATE가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_update(&bound)?,
        ExecuteResult::Command { affected_rows: 1 }
    ));

    let BoundStatement::Select(bound) =
        bind_sql("SELECT * FROM users;", database.catalog.metadata())
    else {
        panic!("SELECT가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_select(&bound)?,
        ExecuteResult::Rows(rows)
            if rows == vec![vec![Value::BigInt(1), Value::Varchar("Lee".to_owned())]]
    ));

    let BoundStatement::Delete(bound) = bind_sql(
        "DELETE FROM users WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("DELETE가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_delete(&bound)?,
        ExecuteResult::Command { affected_rows: 1 }
    ));

    let BoundStatement::Select(bound) =
        bind_sql("SELECT * FROM users;", database.catalog.metadata())
    else {
        panic!("SELECT가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_select(&bound)?,
        ExecuteResult::Rows(rows) if rows.is_empty()
    ));
    Ok(())
}
