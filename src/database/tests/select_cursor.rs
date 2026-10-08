use crate::{
    database::{
        Database, DatabaseError, ExecuteResult,
        select_cursor::{CursorState, SelectCursor},
    },
    executor::{Executor, ExecutorError, QueryRow},
    query::binder::{BoundFromClause, BoundStatement},
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{bind_sql, parse_sql};

fn assert_sql_rows(
    database: &mut Database,
    query: &str,
    expected: &[Vec<Value>],
) -> Result<(), DatabaseError> {
    let ExecuteResult::Rows(mut cursor) = database.execute(&parse_sql(query))? else {
        panic!("SELECT는 커서를 반환해야 함");
    };
    for values in expected {
        assert_eq!(cursor.next_row()?, Some(values.clone()), "{query}");
    }
    assert_eq!(cursor.next_row()?, None, "{query}");
    assert_eq!(cursor.next_row()?, None, "{query}");
    Ok(())
}

#[test]
fn 빈_테이블은_일반_결과_없이_종료하고_집계_결과를_한번만_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("empty-table-select-cursor");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT);"))?;
    }
    let mut database = Database::open(directory.path(), "reopened")?;
    assert_sql_rows(&mut database, "SELECT id FROM users;", &[])?;
    assert_sql_rows(
        &mut database,
        "SELECT COUNT(*), SUM(id) FROM users;",
        &[vec![Value::BigInt(0), Value::Null]],
    )?;
    Ok(())
}

#[test]
fn null_행의_집계는_count에_포함하고_sum은_null을_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("null-aggregate-select-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT);"))?;
    for _ in 0..2 {
        database.execute(&parse_sql("INSERT INTO users VALUES (NULL);"))?;
    }
    assert_sql_rows(
        &mut database,
        "SELECT COUNT(*), SUM(id) FROM users;",
        &[vec![Value::BigInt(2), Value::Null]],
    )?;
    assert_sql_rows(
        &mut database,
        "SELECT COUNT(*), SUM(id) FROM users WHERE id > 0;",
        &[vec![Value::BigInt(0), Value::Null]],
    )?;
    Ok(())
}

#[test]
fn 재시작후_스트리밍_집계는_버퍼_계산과_같은_결과를_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("streaming-buffered-aggregate-parity");
    {
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&parse_sql("CREATE TABLE users (id BIGINT, amount BIGINT);"))?;
        for values in ["10, 7", "NULL, 9", "3, NULL"] {
            database.execute(&parse_sql(&format!("INSERT INTO users VALUES ({values});")))?;
        }
    }
    let mut database = Database::open(directory.path(), "reopened")?;
    for (query, expected) in [
        (
            "SELECT SUM(amount), COUNT(*), SUM(id) FROM users;",
            vec![Value::BigInt(16), Value::BigInt(3), Value::BigInt(13)],
        ),
        (
            "SELECT SUM(amount), COUNT(*), SUM(id) FROM users WHERE id > 5 LIMIT 1;",
            vec![Value::BigInt(7), Value::BigInt(1), Value::BigInt(10)],
        ),
        (
            "SELECT SUM(amount), COUNT(*), SUM(id) FROM users WHERE id > 100;",
            vec![Value::Null, Value::BigInt(0), Value::Null],
        ),
    ] {
        let BoundStatement::Select(bound) = bind_sql(query, database.catalog.metadata()) else {
            panic!("SELECT를 bind해야 함");
        };
        let BoundFromClause::Table(table) = &bound.from else {
            panic!("단일 테이블 SELECT여야 함");
        };
        let raw_rows = database
            .table_manager
            .scan_rows(&mut database.storage_manager, table.table_id)?;
        let executor = Executor::new(database.catalog.metadata());
        let input = executor
            .decode_table_rows(table, raw_rows)?
            .into_iter()
            .map(|row| QueryRow::new(vec![row]))
            .collect();
        let buffered = executor.select_rows(input, &bound)?;
        assert_eq!(buffered, vec![expected]);

        let ExecuteResult::Rows(mut cursor) = database.execute_select(bound)? else {
            panic!("스트리밍 커서를 반환해야 함");
        };
        for values in buffered {
            assert_eq!(cursor.next_row()?, Some(values));
        }
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(cursor.next_row()?, None);
    }
    Ok(())
}

#[test]
fn 스트리밍_sum_overflow는_버퍼와_같은_오류를_전달하고_다음_쿼리를_막지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("streaming-sum-overflow-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    for id in [i64::MAX, 1] {
        database.execute(&parse_sql(&format!(
            "INSERT INTO users VALUES ({id}, 'group');"
        )))?;
    }
    let BoundStatement::Select(bound) = bind_sql(
        "SELECT COUNT(*), SUM(id) FROM users;",
        database.catalog.metadata(),
    ) else {
        panic!("집계 SELECT를 bind해야 함");
    };
    let BoundFromClause::Table(table) = &bound.from else {
        panic!("단일 테이블 SELECT여야 함");
    };
    let raw_rows = database
        .table_manager
        .scan_rows(&mut database.storage_manager, table.table_id)?;
    let executor = Executor::new(database.catalog.metadata());
    let input = executor
        .decode_table_rows(table, raw_rows)?
        .into_iter()
        .map(|row| QueryRow::new(vec![row]))
        .collect();
    assert!(matches!(
        executor.select_rows(input, &bound),
        Err(ExecutorError::SumOverflow)
    ));

    {
        let ExecuteResult::Rows(mut cursor) = database.execute_select(bound)? else {
            panic!("スト리밍 커서를 반환해야 함");
        };
        assert!(matches!(
            cursor.next_row(),
            Err(DatabaseError::Executor(ExecutorError::SumOverflow))
        ));
    }
    assert_sql_rows(
        &mut database,
        "SELECT COUNT(*) FROM users;",
        &[vec![Value::BigInt(2)]],
    )?;
    assert_sql_rows(&mut database, "SELECT SUM(id) FROM users LIMIT 0;", &[])?;
    assert_sql_rows(
        &mut database,
        "SELECT name, SUM(id) FROM users GROUP BY name LIMIT 0;",
        &[],
    )?;
    {
        let ExecuteResult::Rows(mut cursor) = database.execute(&parse_sql(
            "SELECT name, SUM(id) FROM users GROUP BY name LIMIT 1;",
        ))?
        else {
            panic!("GROUP BY도 커서를 반환해야 함");
        };
        assert!(matches!(
            cursor.next_row(),
            Err(DatabaseError::Executor(ExecutorError::SumOverflow))
        ));
    }
    assert_sql_rows(
        &mut database,
        "SELECT name, COUNT(*) FROM users GROUP BY name;",
        &[vec![Value::Varchar("group".to_owned()), Value::BigInt(2)]],
    )?;
    Ok(())
}

#[test]
fn group_cursor는_null_키와_빈_입력을_처리한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("group-null-empty-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    assert_sql_rows(
        &mut database,
        "SELECT name, COUNT(*), SUM(id) FROM users GROUP BY name;",
        &[],
    )?;
    for values in ["NULL, NULL", "NULL, 'A'", "7, 'A'", "NULL, NULL"] {
        database.execute(&parse_sql(&format!("INSERT INTO users VALUES ({values});")))?;
    }
    assert_sql_rows(
        &mut database,
        "SELECT name, COUNT(*), SUM(id) FROM users GROUP BY name;",
        &[
            vec![Value::Null, Value::BigInt(2), Value::Null],
            vec![
                Value::Varchar("A".to_owned()),
                Value::BigInt(2),
                Value::BigInt(7),
            ],
        ],
    )?;
    assert_sql_rows(
        &mut database,
        "SELECT name, COUNT(*) FROM users WHERE id > 100 GROUP BY name;",
        &[],
    )?;
    assert_sql_rows(
        &mut database,
        "SELECT name FROM users GROUP BY name;",
        &[vec![Value::Null], vec![Value::Varchar("A".to_owned())]],
    )?;
    Ok(())
}

#[test]
fn order_by와_group_by의_버퍼_결과는_계산을_다시_적용하지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("buffered-sql-select-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    for values in ["1, 'a'", "2, 'a'", "3, 'b'"] {
        database.execute(&parse_sql(&format!("INSERT INTO users VALUES ({values});")))?;
    }
    assert_sql_rows(
        &mut database,
        "SELECT name FROM users WHERE id > 1 ORDER BY id DESC LIMIT 1;",
        &[vec![Value::Varchar("b".to_owned())]],
    )?;
    assert_sql_rows(
        &mut database,
        "SELECT name, COUNT(*), SUM(id) FROM users WHERE id > 0 GROUP BY name ORDER BY name LIMIT 1;",
        &[vec![
            Value::Varchar("a".to_owned()),
            Value::BigInt(2),
            Value::BigInt(3),
        ]],
    )?;
    Ok(())
}

#[test]
fn buffered_cursor는_행을_소진한_뒤_계속_none을_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("buffered-select-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    let first = vec![Value::BigInt(1)];
    let second = vec![Value::BigInt(2)];
    let mut cursor = SelectCursor::new(
        &mut database,
        CursorState::Buffered(vec![first.clone(), second.clone()].into_iter()),
    );

    assert_eq!(cursor.next_row()?, Some(first));
    assert_eq!(cursor.next_row()?, Some(second));
    assert_eq!(cursor.next_row()?, None);
    assert_eq!(cursor.next_row()?, None);
    Ok(())
}
