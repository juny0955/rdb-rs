use crate::{
    database::{Database, DatabaseError, ExecuteResult},
    query::binder::BinderError,
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{collect_select_rows, parse_sql};

fn seed_join_tables(database: &mut Database) -> Result<(), DatabaseError> {
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql(
        "CREATE TABLE orders (user_id BIGINT, amount BIGINT);",
    ))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (2, 'Lee');"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;
    database.execute(&parse_sql("INSERT INTO orders VALUES (1, 10);"))?;
    database.execute(&parse_sql("INSERT INTO orders VALUES (1, 20);"))?;
    database.execute(&parse_sql("INSERT INTO orders VALUES (NULL, 30);"))?;
    database.execute(&parse_sql("INSERT INTO orders VALUES (3, 40);"))?;
    Ok(())
}

#[test]
fn join은_다중_매칭을_반환하고_null과_불일치를_제외한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-multiple-matches");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id ORDER BY o.amount;",
    ))?;

    // Then
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(10)],
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(20)],
        ]
    );
    Ok(())
}

#[test]
fn 별칭_없는_join에서_테이블_이름으로_컬럼을_참조한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-table-name-qualification");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id ORDER BY orders.amount;",
    ))?;

    // Then
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(10)],
            vec![Value::Varchar("Kim".to_owned()), Value::BigInt(20)],
        ]
    );
    Ok(())
}

#[test]
fn 단일_테이블에서도_테이블_이름으로_컬럼을_참조한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("single-table-name-qualification");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;

    // When
    let result = database.execute(&parse_sql("SELECT users.id FROM users;"))?;

    // Then
    assert_eq!(collect_select_rows(result)?, vec![vec![Value::BigInt(1)]]);
    Ok(())
}

#[test]
fn 중복_별칭은_조인_전에_거부한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-duplicate-alias");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT x.id FROM users x JOIN users x ON x.id = x.id;",
    ));

    // Then
    assert!(matches!(
        result,
        Err(DatabaseError::Binder(BinderError::DuplicateTableReference(name))) if name == "x"
    ));
    Ok(())
}

#[test]
fn 별칭과_테이블_이름의_충돌도_거부한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-alias-table-name-collision");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT * FROM users JOIN orders users ON users.id = users.user_id;",
    ));

    // Then
    assert!(matches!(
        result,
        Err(DatabaseError::Binder(BinderError::DuplicateTableReference(name))) if name == "users"
    ));
    Ok(())
}

#[test]
fn 서로_다른_별칭의_self_join은_각_인스턴스를_구분한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-distinct-aliases");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "SELECT a.name, b.name FROM users a JOIN users b ON a.id = b.id ORDER BY a.id;",
    ))?;

    // Then
    assert_eq!(
        collect_select_rows(result)?,
        vec![
            vec![
                Value::Varchar("Kim".to_owned()),
                Value::Varchar("Kim".to_owned())
            ],
            vec![
                Value::Varchar("Lee".to_owned()),
                Value::Varchar("Lee".to_owned())
            ],
        ]
    );
    Ok(())
}

#[test]
fn join_update는_일치하는_행을_한번만_수정한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-update");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "UPDATE users u JOIN orders o ON u.id = o.user_id SET u.name = 'New';",
    ))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::AffectedRows { affected_rows: 1 }
    ));
    let rows = database.execute(&parse_sql("SELECT name FROM users WHERE id = 1;"))?;
    assert_eq!(
        collect_select_rows(rows)?,
        vec![vec![Value::Varchar("New".to_owned())]]
    );
    Ok(())
}

#[test]
fn join_delete는_일치하는_행을_한번만_삭제한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-delete");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "DELETE u FROM users u JOIN orders o ON u.id = o.user_id;",
    ))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::AffectedRows { affected_rows: 1 }
    ));
    let rows = database.execute(&parse_sql("SELECT id FROM users ORDER BY id;"))?;
    assert_eq!(
        collect_select_rows(rows)?,
        vec![vec![Value::BigInt(2)], vec![Value::Null]]
    );
    Ok(())
}

#[test]
fn join_delete는_별칭_없이_테이블_이름으로_대상을_지정한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("join-delete-table-name-target");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;

    // When
    let result = database.execute(&parse_sql(
        "DELETE users FROM users JOIN orders ON users.id = orders.user_id;",
    ))?;

    // Then
    assert!(matches!(
        result,
        ExecuteResult::AffectedRows { affected_rows: 1 }
    ));
    let rows = database.execute(&parse_sql("SELECT id FROM users ORDER BY id;"))?;
    assert_eq!(
        collect_select_rows(rows)?,
        vec![vec![Value::BigInt(2)], vec![Value::Null]]
    );
    Ok(())
}
