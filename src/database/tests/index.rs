use crate::{
    binder::BoundStatement,
    catalog::metadata::{RelationId, TableId},
    database::{Database, DatabaseError, ExecuteResult, index::search_index_row_ids},
    index::btree::{BTreeKey, BTreeKeyType, tree::BTree},
    storage::page::PageId,
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{bind_sql, parse_sql};

#[test]
fn create_index는_index_file과_metadata를_생성하고_재시작후에도_유지한다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-create-index");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;

        assert!(directory.path().join("1.idx").exists());
    }

    let database = Database::open(directory.path(), "reopened")?;
    let index = database
        .metadata
        .index("idx_users_id")
        .expect("재시작 후 index metadata가 있어야 함");

    assert_eq!(index.root_page_id(), PageId::new(0));
    Ok(())
}

#[test]
fn 재시작후_index_scan은_중복_key의_모든_row를_반환한다() -> Result<(), DatabaseError> {
    // Given
    let directory = TestDirectory::new("database-index-scan-duplicates");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Lee');"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (7, 'Park');"))?;
    }
    let mut database = Database::open(directory.path(), "reopened")?;

    // When
    let result = database.execute(&parse_sql("SELECT name FROM users WHERE id = 42;"))?;

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
fn indexed_equal_predicate는_index_candidate를_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-index-candidate");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
    database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;

    let BoundStatement::Select(bound) =
        bind_sql("SELECT name FROM users WHERE id = 42;", &database.metadata)
    else {
        panic!("SELECT 문이어야 함");
    };

    let candidates = search_index_row_ids(
        &database.metadata,
        &mut database.storage_manager,
        &database.data_dir,
        &bound,
    )?;

    assert!(matches!(candidates, Some(row_ids) if row_ids.len() == 1));
    Ok(())
}

#[test]
fn create_index는_재시작후_기존_행을_backfill한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-create-index-backfill");
    let indexed_row_id = {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

        let heap_table = database.table_cache.get_or_open_table(
            &mut database.storage_manager,
            database.data_dir.as_path(),
            TableId::new(1),
        )?;
        heap_table
            .scan(&mut database.storage_manager)?
            .into_iter()
            .next()
            .expect("인덱싱할 행이 있어야 함")
            .0
    };

    {
        let mut database = Database::open(directory.path(), "reopened")?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
    }

    let mut database = Database::open(directory.path(), "reopened-again")?;
    let index = database
        .metadata
        .index("idx_users_id")
        .expect("index metadata가 있어야 함");
    database.storage_manager.register_relation(
        RelationId::Index(index.id()),
        directory.path().join(format!("{}.idx", index.id().id())),
    );
    let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

    assert_eq!(
        btree.search(&mut database.storage_manager, BTreeKey::BigInt(42))?,
        vec![indexed_row_id]
    );
    Ok(())
}

#[test]
fn insert는_생성된_index에_반영하고_재시작후_검색된다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-insert-index-maintenance");
    let inserted_row_id = {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;

        let heap_table = database.table_cache.get_or_open_table(
            &mut database.storage_manager,
            database.data_dir.as_path(),
            TableId::new(1),
        )?;
        heap_table
            .scan(&mut database.storage_manager)?
            .into_iter()
            .next()
            .expect("삽입한 행이 있어야 함")
            .0
    };

    let mut database = Database::open(directory.path(), "reopened")?;
    let index = database
        .metadata
        .index("idx_users_id")
        .expect("index metadata가 있어야 함");
    database.storage_manager.register_relation(
        RelationId::Index(index.id()),
        directory.path().join(format!("{}.idx", index.id().id())),
    );
    let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

    assert_eq!(
        btree.search(&mut database.storage_manager, BTreeKey::BigInt(42))?,
        vec![inserted_row_id]
    );
    Ok(())
}

#[test]
fn delete는_index_entry를_제거하고_재시작후_검색되지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-delete-index-maintenance");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
        database.execute(&parse_sql("DELETE FROM users WHERE id = 42;"))?;
    }

    let mut database = Database::open(directory.path(), "reopened")?;
    let index = database
        .metadata
        .index("idx_users_id")
        .expect("index metadata가 있어야 함");
    database.storage_manager.register_relation(
        RelationId::Index(index.id()),
        directory.path().join(format!("{}.idx", index.id().id())),
    );
    let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

    assert_eq!(
        btree.search(&mut database.storage_manager, BTreeKey::BigInt(42))?,
        vec![]
    );
    Ok(())
}

#[test]
fn indexed_null_value를_delete하면_index_entry_없음_오류가_발생하지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-delete-null-indexed-value");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

        assert!(matches!(
            database.execute(&parse_sql("DELETE FROM users WHERE name = 'Null';"))?,
            ExecuteResult::Command { affected_rows: 1 }
        ));
    }

    let mut database = Database::open(directory.path(), "reopened")?;
    assert!(matches!(
        database.execute(&parse_sql("SELECT name FROM users;"))?,
        ExecuteResult::Rows(rows) if rows.is_empty()
    ));
    Ok(())
}

#[test]
fn update는_index_entry를_교체하고_재시작후_검색된다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-update-index-maintenance");
    let row_id = {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
        database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;

        let heap_table = database.table_cache.get_or_open_table(
            &mut database.storage_manager,
            database.data_dir.as_path(),
            TableId::new(1),
        )?;
        let row_id = heap_table
            .scan(&mut database.storage_manager)?
            .into_iter()
            .next()
            .expect("수정할 행이 있어야 함")
            .0;

        database.execute(&parse_sql("UPDATE users SET id = 100 WHERE id = 42;"))?;
        row_id
    };

    let mut database = Database::open(directory.path(), "reopened")?;
    let index = database
        .metadata
        .index("idx_users_id")
        .expect("index metadata가 있어야 함");
    database.storage_manager.register_relation(
        RelationId::Index(index.id()),
        directory.path().join(format!("{}.idx", index.id().id())),
    );
    let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

    assert_eq!(
        btree.search(&mut database.storage_manager, BTreeKey::BigInt(42))?,
        vec![]
    );
    assert_eq!(
        btree.search(&mut database.storage_manager, BTreeKey::BigInt(100))?,
        vec![row_id]
    );
    Ok(())
}
