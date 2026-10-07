use crate::{
    database::{Database, DatabaseError, ExecuteResult},
    query::sql::{Parser, ast::Statement, lexer::Lexer},
    test_supports::TestDirectory,
    tuple::Value,
};

use super::{SelectCursor, SelectSource};

fn sql(input: &str) -> Statement {
    let tokens = Lexer::new(input).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}

fn populate_three_pages(database: &mut Database) -> Result<(), DatabaseError> {
    database.execute(&sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    let payload = "x".repeat(5000);
    for id in [1, 2, 3] {
        database.execute(&sql(&format!(
            "INSERT INTO users VALUES ({id}, '{payload}');"
        )))?;
    }
    Ok(())
}

fn cursor<'a>(
    database: &'a mut Database,
    query: &str,
) -> Result<Box<SelectCursor<'a>>, DatabaseError> {
    let ExecuteResult::Rows(cursor) = database.execute(&sql(query))? else {
        panic!("SELECT는 커서를 반환해야 함");
    };
    let SelectSource::Scan { scan, .. } = &cursor.source else {
        panic!("단일 테이블 SELECT는 Scan 경로여야 함");
    };
    assert_eq!(scan.page_count, 3, "입력은 실제로 세 페이지를 사용해야 함");
    assert_eq!(scan.next_page, 0);
    Ok(cursor)
}

fn next_page(cursor: &SelectCursor<'_>) -> u64 {
    let SelectSource::Scan { scan, .. } = &cursor.source else {
        panic!("Scan 상태가 유지되어야 함");
    };
    scan.next_page
}

#[test]
fn scan_limit_one은_첫_결과_반환부터_추가_페이지를_읽지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("scan-limit-one-pages");
    let mut database = Database::open(directory.path(), "test")?;
    populate_three_pages(&mut database)?;
    let mut cursor = cursor(&mut database, "SELECT id FROM users WHERE id > 0 LIMIT 1;")?;

    assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(1)]));
    assert_eq!(next_page(&cursor), 1);
    for _ in 0..2 {
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(next_page(&cursor), 1);
    }
    Ok(())
}

#[test]
fn scan_where_limit은_페이지를_넘어_입력을_공급한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("scan-where-limit-pages");
    let mut database = Database::open(directory.path(), "test")?;
    populate_three_pages(&mut database)?;
    let mut cursor = cursor(&mut database, "SELECT id FROM users WHERE id > 1 LIMIT 2;")?;

    assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(2)]));
    assert_eq!(next_page(&cursor), 2);
    assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(3)]));
    assert_eq!(next_page(&cursor), 3);
    for _ in 0..2 {
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(next_page(&cursor), 3);
    }
    Ok(())
}

#[test]
fn scan_limit_zero는_일반과_집계_모두_페이지를_읽지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("scan-limit-zero-pages");
    let mut database = Database::open(directory.path(), "test")?;
    populate_three_pages(&mut database)?;

    for query in [
        "SELECT id FROM users LIMIT 0;",
        "SELECT COUNT(*), SUM(id) FROM users LIMIT 0;",
    ] {
        let mut cursor = cursor(&mut database, query)?;
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_eq!(next_page(&cursor), 0);
        }
    }
    Ok(())
}

#[test]
fn aggregate_limit_one은_첫_결과_전에_전체_페이지를_누적한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("aggregate-limit-one-pages");
    let mut database = Database::open(directory.path(), "test")?;
    populate_three_pages(&mut database)?;
    let mut cursor = cursor(
        &mut database,
        "SELECT COUNT(*), SUM(id) FROM users LIMIT 1;",
    )?;

    assert_eq!(
        cursor.next_row()?,
        Some(vec![Value::BigInt(3), Value::BigInt(6)])
    );
    assert_eq!(next_page(&cursor), 3);
    for _ in 0..2 {
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(next_page(&cursor), 3);
    }
    Ok(())
}

#[test]
fn scan과_집계는_첫_빈_페이지를_eof로_취급하지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("scan-empty-first-page");
    let mut database = Database::open(directory.path(), "test")?;
    populate_three_pages(&mut database)?;
    database.execute(&sql("DELETE FROM users WHERE id = 1;"))?;

    {
        let mut cursor = cursor(&mut database, "SELECT id FROM users;")?;
        assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(2)]));
        assert_eq!(next_page(&cursor), 2);
        assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(3)]));
        assert_eq!(next_page(&cursor), 3);
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(cursor.next_row()?, None);
    }
    let mut cursor = cursor(&mut database, "SELECT COUNT(*), SUM(id) FROM users;")?;
    assert_eq!(
        cursor.next_row()?,
        Some(vec![Value::BigInt(2), Value::BigInt(5)])
    );
    assert_eq!(next_page(&cursor), 3);
    assert_eq!(cursor.next_row()?, None);
    assert_eq!(cursor.next_row()?, None);
    Ok(())
}
