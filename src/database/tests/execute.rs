use crate::{
    database::{Database, DatabaseError, ExecuteResult},
    query::binder::BoundStatement,
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{bind_sql, collect_select_rows, parse_sql};

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

    assert_eq!(collect_select_rows(database.execute(&select)?)?, expected);

    std::fs::rename(
        directory.path().join("1.tbl"),
        directory.path().join("cached-1.tbl"),
    )
    .expect("첫 SELECT 후 table file 이름을 변경해야 함");

    assert_eq!(collect_select_rows(database.execute(&select)?)?, expected);
    Ok(())
}

#[test]
fn select_sum은_sql부터_heap_scan까지_합계를_반환한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-select-sum");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (3, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Park');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT SUM(id) FROM users;"))?;

    // Then
    assert_eq!(collect_select_rows(result)?, vec![vec![Value::BigInt(13)]]);
    Ok(())
}

#[test]
fn select_count_all_group_by는_sql부터_heap_scan까지_그룹별_개수를_반환한다()
-> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-select-count-group-by");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (2, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (3, 'Kim');"))?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT name, COUNT(*) FROM users GROUP BY name;",
    ))?;

    // Then
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(2)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(1)],
        ]
    );
    Ok(())
}

#[test]
fn select_sum_group_by는_sql부터_heap_scan까지_그룹별_합계를_반환한다() -> Result<(), DatabaseError>
{
    // Given
    let directory = TestDirectory::new("database-select-sum-group-by");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (10, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (3, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Kim');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT name, SUM(id) FROM users GROUP BY name;"))?;

    // Then
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(10)],
            vec![Value::Varchar("Lee".to_owned()), Value::BigInt(3)],
        ]
    );
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
    assert_eq!(
        collect_select_rows(result)?,
        vec![vec![Value::Varchar("Kim".to_owned())]]
    );
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
    assert_eq!(
        collect_select_rows(result)?,
        vec![vec![Value::Varchar("Lee".to_owned())]]
    );
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
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
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
    assert_eq!(
        collect_select_rows(result)?,
        vec![vec![Value::Varchar("Park".to_owned())]]
    );
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
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Lee".to_owned())],
            vec![Value::Varchar("Park".to_owned())],
        ]
    );
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
    database.create_table(bound)?;

    let BoundStatement::Insert(bound) = bind_sql(
        "INSERT INTO users VALUES (1, 'Kim');",
        database.catalog.metadata(),
    ) else {
        panic!("INSERT가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_insert(bound)?,
        ExecuteResult::Success
    ));

    let BoundStatement::Select(bound) = bind_sql(
        "SELECT name FROM users WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("SELECT가 bind되어야 함");
    };
    assert_eq!(
        collect_select_rows(database.execute_select(bound)?)?,
        vec![vec![Value::Varchar("Kim".to_owned())]]
    );

    let BoundStatement::Update(bound) = bind_sql(
        "UPDATE users SET name = 'Lee' WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("UPDATE가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_update(bound)?,
        ExecuteResult::AffectedRows { affected_rows: 1 }
    ));

    let BoundStatement::Select(bound) =
        bind_sql("SELECT * FROM users;", database.catalog.metadata())
    else {
        panic!("SELECT가 bind되어야 함");
    };
    assert_eq!(
        collect_select_rows(database.execute_select(bound)?)?,
        vec![vec![Value::BigInt(1), Value::Varchar("Lee".to_owned())]]
    );

    let BoundStatement::Delete(bound) = bind_sql(
        "DELETE FROM users WHERE id = 1;",
        database.catalog.metadata(),
    ) else {
        panic!("DELETE가 bind되어야 함");
    };
    assert!(matches!(
        database.execute_delete(bound)?,
        ExecuteResult::AffectedRows { affected_rows: 1 }
    ));

    let BoundStatement::Select(bound) =
        bind_sql("SELECT * FROM users;", database.catalog.metadata())
    else {
        panic!("SELECT가 bind되어야 함");
    };
    assert!(collect_select_rows(database.execute_select(bound)?)?.is_empty());
    Ok(())
}
