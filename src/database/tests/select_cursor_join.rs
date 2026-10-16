use crate::{
    database::{
        Database, DatabaseError, ExecuteResult,
        cursor::{
            SelectCursor,
            state::{InputState, join::nested_loop::NestedLoopJoin},
        },
    },
    executor::ExecutorError,
    query::sql::{Parser, ast::Statement, lexer::Lexer},
    test_supports::TestDirectory,
    tuple::Value,
};

use super::HashJoin;

fn sql(input: &str) -> Statement {
    let tokens = Lexer::new(input).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}

fn text(value: &str) -> Value {
    Value::Varchar(value.to_owned())
}

fn cursor<'a>(
    database: &'a mut Database,
    query: &str,
) -> Result<Box<SelectCursor<'a>>, DatabaseError> {
    let ExecuteResult::Rows(cursor) = database.execute(&sql(query))? else {
        panic!("SELECT는 커서를 반환해야 함");
    };
    Ok(cursor)
}

fn hash_join(input: &InputState) -> &HashJoin {
    let InputState::Join(join) = input else {
        panic!("Join 입력이어야 함");
    };
    join.as_any()
        .downcast_ref::<HashJoin>()
        .expect("HashJoin 구현체를 사용하는 입력이어야 함")
}

fn join<'a>(cursor: &'a SelectCursor<'_>) -> &'a HashJoin {
    hash_join(&cursor.input)
}

fn seq_progress(input: &InputState) -> (u64, u64) {
    let InputState::SeqScan(scan) = input else {
        panic!("SeqScan 입력이어야 함");
    };
    scan.page_progress()
}

fn remaining_ids(input: &InputState) -> usize {
    let InputState::IndexScan(scan) = input else {
        panic!("IndexScan 입력이어야 함");
    };
    scan.remaining_row_ids()
}

fn assert_pages(cursor: &SelectCursor<'_>, left: u64, right: u64, built: bool) {
    let join = join(cursor);
    assert_eq!(seq_progress(&join.left), (left, 3));
    assert_eq!(seq_progress(&join.right), (right, 3));
    assert_eq!(join.right_built, built);
}

fn assert_rows(
    cursor: &mut SelectCursor<'_>,
    expected: &[Vec<Value>],
) -> Result<(), DatabaseError> {
    for row in expected {
        assert_eq!(cursor.next_row()?, Some(row.clone()));
    }
    assert_eq!(cursor.next_row()?, None);
    assert_eq!(cursor.next_row()?, None);
    Ok(())
}

fn seed_pages(database: &mut Database) -> Result<(), DatabaseError> {
    database.execute(&sql("CREATE TABLE users (id BIGINT, payload VARCHAR);"))?;
    database.execute(&sql(
        "CREATE TABLE orders (user_id BIGINT, amount BIGINT, payload VARCHAR);",
    ))?;
    let payload = "x".repeat(5000);
    for id in [99, 7, 7] {
        database.execute(&sql(&format!(
            "INSERT INTO users VALUES ({id}, '{payload}');"
        )))?;
    }
    for (id, amount) in [(7, 10), (7, 20), (9, 90)] {
        database.execute(&sql(&format!(
            "INSERT INTO orders VALUES ({id}, {amount}, '{payload}');"
        )))?;
    }
    Ok(())
}

fn seed_indexed(database: &mut Database) -> Result<(), DatabaseError> {
    database.execute(&sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&sql("CREATE TABLE orders (user_id BIGINT, amount BIGINT);"))?;
    database.execute(&sql("CREATE INDEX users_id ON users(id);"))?;
    database.execute(&sql("CREATE INDEX orders_user_id ON orders(user_id);"))?;
    for values in ["7, 'A'", "7, 'B'", "9, 'C'", "NULL, 'N'"] {
        database.execute(&sql(&format!("INSERT INTO users VALUES ({values});")))?;
    }
    for values in ["7, 10", "7, 20", "9, 30", "NULL, 40"] {
        database.execute(&sql(&format!("INSERT INTO orders VALUES ({values});")))?;
    }
    Ok(())
}

#[test]
fn join_cursor는_재시작후_오른쪽을_한번_구축하고_왼쪽_페이지를_필요할때_읽는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-lazy-pages");
    {
        let mut database = Database::open(directory.path(), "test")?;
        seed_pages(&mut database)?;
    }
    let mut database = Database::open(directory.path(), "reopened")?;
    let mut cursor = cursor(
        &mut database,
        "SELECT u.id, o.amount FROM users u JOIN orders o ON u.id = o.user_id;",
    )?;
    assert_pages(&cursor, 0, 0, false);

    for (amount, left_page) in [(10, 2), (20, 2), (10, 3), (20, 3)] {
        assert_eq!(
            cursor.next_row()?,
            Some(vec![Value::BigInt(7), Value::BigInt(amount)])
        );
        assert_pages(&cursor, left_page, 3, true);
    }
    for _ in 0..2 {
        assert_eq!(cursor.next_row()?, None);
        assert_pages(&cursor, 3, 3, true);
    }
    Ok(())
}

#[test]
fn join_cursor의_where와_limit_one은_남은_왼쪽_페이지를_읽지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-limit-one");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pages(&mut database)?;
    for (query, amount) in [
        (
            "SELECT o.amount FROM users u JOIN orders o ON u.id = o.user_id LIMIT 1;",
            10,
        ),
        (
            "SELECT o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE o.amount > 10 LIMIT 1;",
            20,
        ),
    ] {
        let mut cursor = cursor(&mut database, query)?;
        assert_pages(&cursor, 0, 0, false);
        assert_eq!(cursor.next_row()?, Some(vec![Value::BigInt(amount)]));
        assert_pages(&cursor, 2, 3, true);
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_pages(&cursor, 2, 3, true);
        }
    }
    Ok(())
}

#[test]
fn join_cursor의_limit_zero는_일반과_집계와_그룹_모두_입력을_읽지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-limit-zero");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pages(&mut database)?;
    for query in [
        "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id LIMIT 0;",
        "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id LIMIT 0;",
        "SELECT u.id, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.id LIMIT 0;",
    ] {
        let mut cursor = cursor(&mut database, query)?;
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_pages(&cursor, 0, 0, false);
        }
    }
    Ok(())
}

#[test]
fn join_cursor의_집계와_group_limit은_모든_매칭을_누적한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-aggregate-group");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pages(&mut database)?;
    for (query, expected) in [
        (
            "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id LIMIT 1;",
            vec![Value::BigInt(4), Value::BigInt(60)],
        ),
        (
            "SELECT SUM(o.amount), u.id, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.id LIMIT 1;",
            vec![Value::BigInt(60), Value::BigInt(7), Value::BigInt(4)],
        ),
        (
            "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id WHERE o.amount > 10;",
            vec![Value::BigInt(2), Value::BigInt(40)],
        ),
    ] {
        let mut cursor = cursor(&mut database, query)?;
        assert_pages(&cursor, 0, 0, false);
        assert_eq!(cursor.next_row()?, Some(expected));
        assert_pages(&cursor, 3, 3, true);
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_pages(&cursor, 3, 3, true);
        }
    }
    Ok(())
}

#[test]
fn join_cursor는_재시작후_좌우_인덱스를_독립적으로_선택하고_잔여_where를_검사한다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-index-selection");
    {
        let mut database = Database::open(directory.path(), "test")?;
        seed_indexed(&mut database)?;
    }
    let mut database = Database::open(directory.path(), "reopened")?;
    for (filter, left_indexed, right_indexed) in [
        ("u.id = 7 AND o.amount > 10", true, false),
        ("o.user_id = 7 AND o.amount > 10", false, true),
        ("u.id = 7 AND o.user_id = 7 AND o.amount > 10", true, true),
    ] {
        let query = format!(
            "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE {filter};"
        );
        let mut cursor = cursor(&mut database, &query)?;
        let state = join(&cursor);
        assert!(!state.right_built);
        for (input, indexed) in [(&*state.left, left_indexed), (&*state.right, right_indexed)] {
            if indexed {
                assert_eq!(remaining_ids(input), 2);
            } else {
                assert_eq!(seq_progress(input).0, 0);
            }
        }
        assert_rows(
            &mut cursor,
            &[
                vec![text("A"), Value::BigInt(20)],
                vec![text("B"), Value::BigInt(20)],
            ],
        )?;
    }
    Ok(())
}

#[test]
fn join_cursor의_index_limit은_row_id_소비를_중단하고_limit_zero는_해시를_구축하지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-index-limits");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    for limit in [0, 1] {
        let query = format!(
            "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id = 7 AND o.user_id = 7 LIMIT {limit};"
        );
        let mut cursor = cursor(&mut database, &query)?;
        assert_eq!(remaining_ids(&join(&cursor).left), 2);
        assert_eq!(remaining_ids(&join(&cursor).right), 2);
        assert!(!join(&cursor).right_built);
        if limit == 1 {
            assert_eq!(cursor.next_row()?, Some(vec![text("A"), Value::BigInt(10)]));
        }
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            let state = join(&cursor);
            assert_eq!(state.right_built, limit == 1);
            assert_eq!(remaining_ids(&state.left), if limit == 0 { 2 } else { 1 });
            assert_eq!(remaining_ids(&state.right), if limit == 0 { 2 } else { 0 });
        }
    }
    Ok(())
}

#[test]
fn join_cursor는_인덱스_매칭이_없어도_빈_index_scan을_유지한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-empty-index");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    for (filter, empty_left) in [
        ("u.id = 999", true),
        ("o.user_id = 999", false),
        ("u.id = NULL", true),
    ] {
        let mut cursor = cursor(
            &mut database,
            &format!(
                "SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id WHERE {filter};"
            ),
        )?;
        let state = join(&cursor);
        assert_eq!(
            remaining_ids(if empty_left {
                &state.left
            } else {
                &state.right
            }),
            0
        );
        assert_rows(&mut cursor, &[])?;
    }
    for query in [
        "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id = 999;",
        "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id WHERE o.user_id = 999;",
    ] {
        let mut cursor = cursor(&mut database, query)?;
        join(&cursor);
        assert_rows(&mut cursor, &[vec![Value::BigInt(0), Value::Null]])?;
    }
    Ok(())
}

#[test]
fn join_cursor는_인덱스가_있어도_on만으로_선택하거나_or를_잘못_축소하지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-index-fallback");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    for suffix in ["", "WHERE u.id = 7 OR o.user_id = 9"] {
        let mut cursor = cursor(
            &mut database,
            &format!(
                "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id {suffix};"
            ),
        )?;
        let state = join(&cursor);
        assert_eq!(seq_progress(&state.left).0, 0);
        assert_eq!(seq_progress(&state.right).0, 0);
        assert_rows(
            &mut cursor,
            &[
                vec![text("A"), Value::BigInt(10)],
                vec![text("A"), Value::BigInt(20)],
                vec![text("B"), Value::BigInt(10)],
                vec![text("B"), Value::BigInt(20)],
                vec![text("C"), Value::BigInt(30)],
            ],
        )?;
    }
    Ok(())
}

#[test]
fn join_cursor는_인덱스나_동등조건이_없으면_seq_scan을_사용한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-no-index");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pages(&mut database)?;
    let mut cursor = cursor(
        &mut database,
        "SELECT o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id = 7;",
    )?;
    assert_pages(&cursor, 0, 0, false);
    assert_rows(
        &mut cursor,
        &[
            vec![Value::BigInt(10)],
            vec![Value::BigInt(20)],
            vec![Value::BigInt(10)],
            vec![Value::BigInt(20)],
        ],
    )?;
    drop(cursor);

    database.execute(&sql("CREATE INDEX users_id ON users(id);"))?;
    let mut cursor = self::cursor(
        &mut database,
        "SELECT o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id > 7;",
    )?;
    assert_pages(&cursor, 0, 0, false);
    assert_rows(&mut cursor, &[])?;
    Ok(())
}

#[test]
fn join_cursor는_반대_on과_self_join에서도_별칭별_인덱스_조건을_구분한다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-self-alias-index");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    for filter in ["a.id = 7", "b.id = 7"] {
        let mut cursor = cursor(
            &mut database,
            &format!(
                "SELECT a.name, b.name FROM users a JOIN users b ON b.id = a.id WHERE {filter};"
            ),
        )?;
        let state = join(&cursor);
        if filter.starts_with('a') {
            assert_eq!(remaining_ids(&state.left), 2);
            assert_eq!(seq_progress(&state.right).0, 0);
        } else {
            assert_eq!(seq_progress(&state.left).0, 0);
            assert_eq!(remaining_ids(&state.right), 2);
        }
        assert_rows(
            &mut cursor,
            &[
                vec![text("A"), text("A")],
                vec![text("A"), text("B")],
                vec![text("B"), text("A")],
                vec![text("B"), text("B")],
            ],
        )?;
    }
    Ok(())
}

#[test]
fn join_cursor는_중첩_조인의_리프에도_인덱스를_적용한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-nested-index");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    database.execute(&sql("CREATE TABLE tags (amount BIGINT, label VARCHAR);"))?;
    for values in ["10, 'X'", "10, 'Y'", "20, 'Z'"] {
        database.execute(&sql(&format!("INSERT INTO tags VALUES ({values});")))?;
    }
    let mut cursor = cursor(
        &mut database,
        "SELECT u.name, t.label FROM users u JOIN orders o ON u.id = o.user_id JOIN tags t ON t.amount = o.amount WHERE u.id = 7 AND o.user_id = 7;",
    )?;
    let inner = hash_join(&join(&cursor).left);
    assert_eq!(remaining_ids(&inner.left), 2);
    assert_eq!(remaining_ids(&inner.right), 2);
    assert!(!inner.right_built);
    assert_rows(
        &mut cursor,
        &[
            vec![text("A"), text("X")],
            vec![text("A"), text("Y")],
            vec![text("A"), text("Z")],
            vec![text("B"), text("X")],
            vec![text("B"), text("Y")],
            vec![text("B"), text("Z")],
        ],
    )?;
    Ok(())
}

#[test]
fn join_cursor는_order_by도_scan으로_같은쪽_on은_nested_loop로_처리한다()
-> Result<(), DatabaseError> {
    enum ExpectedPath {
        Hash,
        NestedLoop,
        HashWithNestedLeft,
    }

    let directory = TestDirectory::new("join-cursor-ordered-scan");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    database.execute(&sql("CREATE TABLE tags (amount BIGINT);"))?;
    database.execute(&sql("INSERT INTO tags VALUES (20);"))?;
    for (query, expected, path) in [
        (
            "SELECT o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id = 7 ORDER BY o.amount DESC;",
            vec![
                vec![Value::BigInt(20)],
                vec![Value::BigInt(20)],
                vec![Value::BigInt(10)],
                vec![Value::BigInt(10)],
            ],
            ExpectedPath::Hash,
        ),
        (
            "SELECT u.name FROM users u JOIN orders o ON u.id = u.id WHERE u.id = 7 AND o.amount = 20;",
            vec![vec![text("A")], vec![text("B")]],
            ExpectedPath::NestedLoop,
        ),
        (
            "SELECT u.name FROM users u JOIN orders o ON o.user_id = o.user_id WHERE u.id = 7 AND o.amount = 20;",
            vec![vec![text("A")], vec![text("B")]],
            ExpectedPath::NestedLoop,
        ),
        (
            "SELECT u.name FROM users u JOIN orders o ON u.id = u.id JOIN tags t ON o.amount = t.amount WHERE u.id = 7;",
            vec![vec![text("A")], vec![text("B")]],
            ExpectedPath::HashWithNestedLeft,
        ),
        (
            "SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id JOIN tags t ON u.id = o.user_id WHERE u.id = 7;",
            vec![
                vec![text("A")],
                vec![text("A")],
                vec![text("B")],
                vec![text("B")],
            ],
            ExpectedPath::NestedLoop,
        ),
    ] {
        let mut cursor = cursor(&mut database, query)?;
        match path {
            ExpectedPath::Hash => {
                assert!(!join(&cursor).right_built, "{query}");
            }
            ExpectedPath::NestedLoop => {
                let InputState::Join(state) = &cursor.input else {
                    panic!("Nested Loop 커서여야 함: {query}");
                };
                assert!(state.as_any().is::<NestedLoopJoin>(), "{query}");
            }
            ExpectedPath::HashWithNestedLeft => {
                let InputState::Join(inner) = &*join(&cursor).left else {
                    panic!("Hash Join의 왼쪽도 조인이어야 함: {query}");
                };
                assert!(inner.as_any().is::<NestedLoopJoin>(), "{query}");
            }
        }
        assert_rows(&mut cursor, &expected)?;
    }
    Ok(())
}

#[test]
fn order_by_join도_같은쪽_on을_nested_loop로_처리한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("ordered-join-same-side-on");
    let mut database = Database::open(directory.path(), "test")?;
    seed_indexed(&mut database)?;
    let mut cursor = cursor(
        &mut database,
        "SELECT u.name FROM users u JOIN orders o ON u.id = u.id WHERE u.id = 7 AND o.amount = 20 ORDER BY u.name;",
    )?;
    let InputState::Join(state) = &cursor.input else {
        panic!("Nested Loop 커서여야 함");
    };
    assert!(state.as_any().is::<NestedLoopJoin>());
    assert_rows(&mut cursor, &[vec![text("A")], vec![text("B")]])?;
    Ok(())
}

#[test]
fn join_cursor는_빈_입력에서_일반과_그룹은_없고_집계는_한번_반환한다() -> Result<(), DatabaseError>
{
    for (left, right) in [(false, false), (true, false), (false, true)] {
        let directory = TestDirectory::new("join-cursor-empty-input");
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&sql("CREATE TABLE users (id BIGINT);"))?;
        database.execute(&sql("CREATE TABLE orders (user_id BIGINT, amount BIGINT);"))?;
        if left {
            database.execute(&sql("INSERT INTO users VALUES (7);"))?;
        }
        if right {
            database.execute(&sql("INSERT INTO orders VALUES (7, 10);"))?;
        }
        for (query, expected) in [
            (
                "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id;",
                vec![],
            ),
            (
                "SELECT u.id, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.id;",
                vec![],
            ),
            (
                "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id;",
                vec![vec![Value::BigInt(0), Value::Null]],
            ),
        ] {
            let mut cursor = cursor(&mut database, query)?;
            join(&cursor);
            assert_rows(&mut cursor, &expected)?;
        }
    }
    Ok(())
}

#[test]
fn join_cursor의_집계_overflow는_전달되고_커서_해제후_다시_조회할수_있다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("join-cursor-overflow");
    let mut database = Database::open(directory.path(), "test")?;
    database.execute(&sql("CREATE TABLE users (id BIGINT);"))?;
    database.execute(&sql("CREATE TABLE orders (user_id BIGINT, amount BIGINT);"))?;
    database.execute(&sql("INSERT INTO users VALUES (7);"))?;
    for amount in [i64::MAX, 1] {
        database.execute(&sql(&format!("INSERT INTO orders VALUES (7, {amount});")))?;
    }
    for query in [
        "SELECT SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id;",
        "SELECT u.id, SUM(o.amount) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.id LIMIT 1;",
    ] {
        let mut cursor = cursor(&mut database, query)?;
        join(&cursor);
        assert!(matches!(
            cursor.next_row(),
            Err(DatabaseError::Executor(ExecutorError::SumOverflow))
        ));
    }
    let mut cursor = cursor(
        &mut database,
        "SELECT COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id;",
    )?;
    assert_rows(&mut cursor, &[vec![Value::BigInt(2)]])?;
    Ok(())
}
