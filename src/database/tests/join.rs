use crate::{
    database::{Database, DatabaseError, ExecuteResult},
    index::IndexManager,
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
fn join_update는_서로다른_테이블의_동일한_row_id를_각각_수정한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-update-multiple-tables");
    let mut database = Database::open(directory.path(), "test")?;
    seed_join_tables(&mut database)?;
    let users_id = database
        .catalog
        .metadata()
        .table("users")
        .expect("users 테이블이 있어야 함")
        .id();
    let orders_id = database
        .catalog
        .metadata()
        .table("orders")
        .expect("orders 테이블이 있어야 함")
        .id();
    let users = super::collect_table_rows(&mut database, users_id)?;
    let orders = super::collect_table_rows(&mut database, orders_id)?;
    assert_eq!(
        users[0].0, orders[0].0,
        "서로 다른 테이블의 RowId가 같아야 함"
    );

    assert!(matches!(
        database.execute(&parse_sql(
            "UPDATE users u JOIN orders o ON u.id = o.user_id SET u.name = 'New', o.amount = 99;",
        ))?,
        ExecuteResult::AffectedRows { affected_rows: 3 }
    ));
    assert_eq!(
        collect_select_rows(database.execute(&parse_sql("SELECT name FROM users WHERE id = 1;"))?)?,
        vec![vec![Value::Varchar("New".to_owned())]]
    );
    assert_eq!(
        collect_select_rows(database.execute(&parse_sql("SELECT name FROM users WHERE id = 2;"))?)?,
        vec![vec![Value::Varchar("Lee".to_owned())]]
    );
    assert_eq!(
        collect_select_rows(
            database.execute(&parse_sql("SELECT amount FROM orders WHERE user_id = 1;"))?
        )?,
        vec![vec![Value::BigInt(99)], vec![Value::BigInt(99)]]
    );
    assert_eq!(
        collect_select_rows(
            database.execute(&parse_sql("SELECT amount FROM orders WHERE user_id = 3;"))?
        )?,
        vec![vec![Value::BigInt(40)]]
    );
    Ok(())
}

#[test]
fn self_join_update는_다음_페이지의_join_key를_바꿔도_대상을_누락하지_않는다()
-> Result<(), DatabaseError> {
    for indexed in [false, true] {
        let directory = TestDirectory::new("self-join-update-across-pages");
        let expected = vec![
            vec![Value::BigInt(1), Value::BigInt(2)],
            vec![Value::BigInt(2), Value::BigInt(99)],
            vec![Value::BigInt(3), Value::BigInt(99)],
        ];
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql(
                "CREATE TABLE nodes (id BIGINT, next_id BIGINT, payload VARCHAR);",
            ))?;
            if indexed {
                database.execute(&parse_sql("CREATE INDEX idx_nodes_next ON nodes(next_id);"))?;
            }
            let payload = "x".repeat(5000);
            for (id, next_id) in [(1, "2"), (2, "3"), (3, "NULL")] {
                database.execute(&parse_sql(&format!(
                    "INSERT INTO nodes VALUES ({id}, {next_id}, '{payload}');"
                )))?;
            }
            let table_id = database
                .catalog
                .metadata()
                .table("nodes")
                .expect("nodes 테이블이 있어야 함")
                .id();
            assert_eq!(
                database
                    .table_manager
                    .page_count(&mut database.storage_manager, table_id)?,
                3,
                "각 원본 행은 서로 다른 페이지에 저장되어야 함"
            );
            assert_eq!(
                collect_select_rows(database.execute(&parse_sql(
                    "SELECT b.id FROM nodes a JOIN nodes b ON a.next_id = b.id ORDER BY b.id;",
                ))?)?,
                vec![vec![Value::BigInt(2)], vec![Value::BigInt(3)]]
            );

            assert!(matches!(
                database.execute(&parse_sql(
                    "UPDATE nodes a JOIN nodes b ON a.next_id = b.id SET b.next_id = 99, b.payload = 'Updated';",
                ))?,
                ExecuteResult::AffectedRows { affected_rows: 2 }
            ));
            assert_eq!(
                collect_select_rows(
                    database.execute(&parse_sql("SELECT id, next_id FROM nodes ORDER BY id;"))?
                )?,
                expected
            );
        }

        let mut database = Database::open(directory.path(), "reopened")?;
        assert_eq!(
            collect_select_rows(
                database.execute(&parse_sql("SELECT id, next_id FROM nodes ORDER BY id;"))?
            )?,
            expected
        );
        assert_eq!(
            collect_select_rows(database.execute(&parse_sql(
                "SELECT id FROM nodes WHERE payload = 'Updated' ORDER BY id;"
            ))?)?,
            vec![vec![Value::BigInt(2)], vec![Value::BigInt(3)]]
        );
        if indexed {
            let metadata = database.catalog.metadata();
            let table = metadata.table("nodes").expect("nodes 테이블이 있어야 함");
            for (key, count) in [(2, 1), (3, 0), (99, 2)] {
                let row_ids = IndexManager::search_index_row_ids(
                    &mut database.storage_manager,
                    metadata.indexes(),
                    table.id(),
                    table.columns()[1].id(),
                    &Value::BigInt(key),
                )?
                .expect("기존 next_id 인덱스로 검색해야 함");
                assert_eq!(
                    row_ids.len(),
                    count,
                    "인덱스 키 {key}의 행 수가 일치해야 함"
                );
            }
        }
    }
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

#[test]
fn self_join_delete는_다음_페이지의_원본을_삭제해도_대상을_누락하지_않는다()
-> Result<(), DatabaseError> {
    for indexed in [false, true] {
        let directory = TestDirectory::new("self-join-delete-across-pages");
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql(
                "CREATE TABLE nodes (id BIGINT, next_id BIGINT, payload VARCHAR);",
            ))?;
            if indexed {
                database.execute(&parse_sql("CREATE INDEX idx_nodes_id ON nodes(id);"))?;
            }
            let payload = "x".repeat(5000);
            for (id, next_id) in [(1, "2"), (2, "3"), (3, "NULL")] {
                database.execute(&parse_sql(&format!(
                    "INSERT INTO nodes VALUES ({id}, {next_id}, '{payload}');"
                )))?;
            }
            let table_id = database
                .catalog
                .metadata()
                .table("nodes")
                .expect("nodes 테이블이 존재해야 함")
                .id();
            assert_eq!(
                database
                    .table_manager
                    .page_count(&mut database.storage_manager, table_id)?,
                3,
                "각 원본 행은 서로 다른 페이지에 저장되어야 함"
            );
            assert_eq!(
                collect_select_rows(database.execute(&parse_sql(
                    "SELECT b.id FROM nodes a JOIN nodes b ON a.next_id = b.id ORDER BY b.id;",
                ))?)?,
                vec![vec![Value::BigInt(2)], vec![Value::BigInt(3)]]
            );

            assert!(matches!(
                database.execute(&parse_sql(
                    "DELETE b FROM nodes a JOIN nodes b ON a.next_id = b.id;",
                ))?,
                ExecuteResult::AffectedRows { affected_rows: 2 }
            ));
            assert_eq!(
                collect_select_rows(
                    database.execute(&parse_sql("SELECT id FROM nodes ORDER BY id;"))?
                )?,
                vec![vec![Value::BigInt(1)]]
            );
        }

        let mut database = Database::open(directory.path(), "reopened")?;
        assert_eq!(
            collect_select_rows(
                database.execute(&parse_sql("SELECT id FROM nodes ORDER BY id;"))?
            )?,
            vec![vec![Value::BigInt(1)]]
        );
        if indexed {
            let metadata = database.catalog.metadata();
            let table = metadata.table("nodes").expect("nodes 테이블이 존재해야 함");
            let column = table.columns()[0].id();
            for id in [1, 2, 3] {
                let row_ids = IndexManager::search_index_row_ids(
                    &mut database.storage_manager,
                    metadata.indexes(),
                    table.id(),
                    column,
                    &Value::BigInt(id),
                )?
                .expect("기존 id 인덱스로 검색해야 함");
                assert_eq!(row_ids.len(), usize::from(id == 1));
            }
        }
    }
    Ok(())
}
