use crate::{
    database::{
        Database, DatabaseError, ExecuteResult,
        cursor::{
            SelectCursor,
            state::{CursorState, InputState, join::hash::HashJoin},
        },
    },
    query::{
        binder::{Binder, BoundFromClause, BoundStatement},
        sql::{Parser, ast::Statement, lexer::Lexer},
    },
    test_supports::TestDirectory,
    tuple::Value,
};

use super::NestedLoopJoin;

fn sql(input: &str) -> Statement {
    let tokens = Lexer::new(input).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}

fn text(value: &str) -> Value {
    Value::Varchar(value.to_owned())
}

fn seed_pairs(database: &mut Database, indexed: bool) -> Result<(), DatabaseError> {
    database.execute(&sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
    database.execute(&sql("CREATE TABLE orders (user_id BIGINT, amount BIGINT);"))?;
    if indexed {
        database.execute(&sql("CREATE INDEX users_id ON users(id);"))?;
        database.execute(&sql("CREATE INDEX orders_user_id ON orders(user_id);"))?;
    }
    for values in ["7, 'A'", "7, 'B'"] {
        database.execute(&sql(&format!("INSERT INTO users VALUES ({values});")))?;
    }
    for amount in [10, 20] {
        database.execute(&sql(&format!("INSERT INTO orders VALUES (7, {amount});")))?;
    }
    Ok(())
}

fn source(database: &mut Database, query: &str) -> Result<InputState, DatabaseError> {
    let BoundStatement::Select(bound) =
        Binder::new(database.catalog.metadata()).bind(&sql(query))?
    else {
        panic!("SELECT를 bind해야 함");
    };
    InputState::from_clause(database, &bound.from, bound.filter.as_ref())
}

fn next_values(
    input: &mut InputState,
    database: &mut Database,
) -> Result<Option<Vec<Value>>, DatabaseError> {
    Ok(input.next_input_row(database)?.map(|row| {
        row.table_rows
            .into_iter()
            .flat_map(|row| row.values)
            .collect()
    }))
}

fn assert_source_rows(
    input: &mut InputState,
    database: &mut Database,
    expected: &[Vec<Value>],
) -> Result<(), DatabaseError> {
    for values in expected {
        assert_eq!(next_values(input, database)?, Some(values.clone()));
    }
    assert_eq!(next_values(input, database)?, None);
    assert_eq!(next_values(input, database)?, None);
    Ok(())
}

fn nested_loop(input: &InputState) -> &NestedLoopJoin {
    let InputState::Join(join) = input else {
        panic!("Join 입력이어야 함");
    };
    join.as_any()
        .downcast_ref::<NestedLoopJoin>()
        .expect("NestedLoopJoin이어야 함")
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

fn cursor_join<'a>(cursor: &'a SelectCursor<'_>) -> &'a NestedLoopJoin {
    let CursorState::Scan { input, .. } = &cursor.state else {
        panic!("Scan 커서여야 함");
    };
    nested_loop(input)
}

fn page_progress(input: &InputState) -> (u64, u64) {
    let InputState::SeqScan(scan) = input else {
        panic!("SeqScan이어야 함");
    };
    scan.page_progress()
}

fn remaining_ids(input: &InputState) -> usize {
    let InputState::IndexScan(scan) = input else {
        panic!("IndexScan이어야 함");
    };
    scan.remaining_row_ids()
}

fn seed_pages(database: &mut Database) -> Result<(), DatabaseError> {
    database.execute(&sql(
        "CREATE TABLE users (id BIGINT, name VARCHAR, payload VARCHAR);",
    ))?;
    database.execute(&sql(
        "CREATE TABLE orders (user_id BIGINT, amount BIGINT, payload VARCHAR);",
    ))?;
    let payload = "x".repeat(5000);
    for values in ["7, 'A'", "9, 'B'", "NULL, 'N'"] {
        database.execute(&sql(&format!(
            "INSERT INTO users VALUES ({values}, '{payload}');"
        )))?;
    }
    for (id, amount) in [(99, 0), (7, 7), (8, 8)] {
        database.execute(&sql(&format!(
            "INSERT INTO orders VALUES ({id}, {amount}, '{payload}');"
        )))?;
    }
    Ok(())
}

#[test]
fn seq와_index_scan은_부분소비와_eof후에도_처음부터_재시작한다() -> Result<(), DatabaseError> {
    for indexed in [false, true] {
        let directory = TestDirectory::new("input-rescan-leaves");
        let mut database = Database::open(directory.path(), "test")?;
        seed_pairs(&mut database, indexed)?;
        let mut input = source(&mut database, "SELECT * FROM users WHERE id = 7;")?;
        let expected = [
            vec![Value::BigInt(7), text("A")],
            vec![Value::BigInt(7), text("B")],
        ];
        if indexed {
            assert_eq!(remaining_ids(&input), 2);
        } else {
            assert_eq!(page_progress(&input), (0, 1));
        }
        input.rescan();
        assert_eq!(
            next_values(&mut input, &mut database)?,
            Some(expected[0].clone())
        );
        input.rescan();
        if indexed {
            assert_eq!(remaining_ids(&input), 2);
        } else {
            assert_eq!(page_progress(&input), (0, 1));
        }
        assert_source_rows(&mut input, &mut database, &expected)?;
        if indexed {
            assert_eq!(remaining_ids(&input), 0);
        }
        input.rescan();
        assert_source_rows(&mut input, &mut database, &expected)?;
    }
    Ok(())
}

#[test]
fn nested_loop는_부분결과와_eof후_좌우입력을_재시작한다() -> Result<(), DatabaseError> {
    for indexed in [false, true] {
        let directory = TestDirectory::new("nested-loop-rescan-inputs");
        let mut database = Database::open(directory.path(), "test")?;
        seed_pairs(&mut database, indexed)?;
        let mut input = source(
            &mut database,
            "SELECT * FROM users u JOIN orders o ON u.id = u.id WHERE u.id = 7 AND o.user_id = 7;",
        )?;
        nested_loop(&input);
        let expected = [
            vec![
                Value::BigInt(7),
                text("A"),
                Value::BigInt(7),
                Value::BigInt(10),
            ],
            vec![
                Value::BigInt(7),
                text("A"),
                Value::BigInt(7),
                Value::BigInt(20),
            ],
            vec![
                Value::BigInt(7),
                text("B"),
                Value::BigInt(7),
                Value::BigInt(10),
            ],
            vec![
                Value::BigInt(7),
                text("B"),
                Value::BigInt(7),
                Value::BigInt(20),
            ],
        ];
        input.rescan();
        assert_eq!(
            next_values(&mut input, &mut database)?,
            Some(expected[0].clone())
        );
        assert!(nested_loop(&input).current_left.is_some());
        input.rescan();
        let join = nested_loop(&input);
        assert!(join.current_left.is_none());
        if indexed {
            assert_eq!(remaining_ids(&join.left), 2);
            assert_eq!(remaining_ids(&join.right), 2);
        } else {
            assert_eq!(page_progress(&join.left), (0, 1));
            assert_eq!(page_progress(&join.right), (0, 1));
        }
        assert_source_rows(&mut input, &mut database, &expected)?;
        input.rescan();
        assert_source_rows(&mut input, &mut database, &expected)?;
    }
    Ok(())
}

#[test]
fn nested_loop는_첫_불일치를_건너뛰고_왼쪽행마다_오른쪽페이지를_재탐색한다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("nested-loop-rescan-pages");
    {
        let mut database = Database::open(directory.path(), "test")?;
        seed_pages(&mut database)?;
    }
    let mut database = Database::open(directory.path(), "reopened")?;
    let mut cursor = cursor(
        &mut database,
        "SELECT u.name, o.amount FROM users u JOIN orders o ON o.user_id = o.amount;",
    )?;
    assert_eq!(page_progress(&cursor_join(&cursor).left), (0, 3));
    assert_eq!(page_progress(&cursor_join(&cursor).right), (0, 3));
    for (name, left_page) in [("A", 1), ("B", 2), ("N", 3)] {
        for (amount, right_page) in [(7, 2), (8, 3)] {
            assert_eq!(
                cursor.next_row()?,
                Some(vec![text(name), Value::BigInt(amount)])
            );
            assert_eq!(page_progress(&cursor_join(&cursor).left), (left_page, 3));
            assert_eq!(page_progress(&cursor_join(&cursor).right), (right_page, 3));
        }
    }
    for _ in 0..2 {
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(page_progress(&cursor_join(&cursor).left), (3, 3));
        assert_eq!(page_progress(&cursor_join(&cursor).right), (3, 3));
    }
    Ok(())
}

#[test]
fn nested_loop의_where와_limit은_필요한_페이지만_읽고_중단한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("nested-loop-limit-pages");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pages(&mut database)?;
    for (suffix, name, left_page) in [("LIMIT 1", "A", 1), ("WHERE u.id > 7 LIMIT 1", "B", 2)] {
        let mut cursor = cursor(
            &mut database,
            &format!(
                "SELECT u.name, o.amount FROM users u JOIN orders o ON o.user_id = o.amount {suffix};"
            ),
        )?;
        assert_eq!(cursor.next_row()?, Some(vec![text(name), Value::BigInt(7)]));
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_eq!(page_progress(&cursor_join(&cursor).left), (left_page, 3));
            assert_eq!(page_progress(&cursor_join(&cursor).right), (2, 3));
        }
    }
    for query in [
        "SELECT u.name FROM users u JOIN orders o ON u.id = u.id LIMIT 0;",
        "SELECT COUNT(*), SUM(o.amount) FROM users u JOIN orders o ON u.id = u.id LIMIT 0;",
        "SELECT u.id, COUNT(*) FROM users u JOIN orders o ON u.id = u.id GROUP BY u.id LIMIT 0;",
    ] {
        let mut cursor = cursor(&mut database, query)?;
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_eq!(page_progress(&cursor_join(&cursor).left), (0, 3));
            assert_eq!(page_progress(&cursor_join(&cursor).right), (0, 3));
        }
    }
    Ok(())
}

#[test]
fn nested_loop의_index_limit은_잔여조건을_검사하고_남은_row_id를_소비하지_않는다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("nested-loop-index-limit");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pairs(&mut database, true)?;
    for limit in [0, 1] {
        let mut cursor = cursor(
            &mut database,
            &format!(
                "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = u.id WHERE u.id = 7 AND o.user_id = 7 AND o.amount > 10 LIMIT {limit};"
            ),
        )?;
        assert_eq!(remaining_ids(&cursor_join(&cursor).left), 2);
        assert_eq!(remaining_ids(&cursor_join(&cursor).right), 2);
        if limit == 1 {
            assert_eq!(cursor.next_row()?, Some(vec![text("A"), Value::BigInt(20)]));
        }
        for _ in 0..2 {
            assert_eq!(cursor.next_row()?, None);
            assert_eq!(
                remaining_ids(&cursor_join(&cursor).left),
                if limit == 0 { 2 } else { 1 }
            );
            assert_eq!(
                remaining_ids(&cursor_join(&cursor).right),
                if limit == 0 { 2 } else { 0 }
            );
        }
    }
    Ok(())
}

#[test]
fn nested_loop는_빈입력과_모든조건_불일치에서도_종료하고_재시작한다() -> Result<(), DatabaseError> {
    for (left, right) in [(false, false), (true, false), (false, true), (true, true)] {
        let directory = TestDirectory::new("nested-loop-empty-input");
        let mut database = Database::open(directory.path(), "test")?;
        database.execute(&sql("CREATE TABLE users (id BIGINT);"))?;
        database.execute(&sql("CREATE TABLE orders (user_id BIGINT, amount BIGINT);"))?;
        if left {
            database.execute(&sql("INSERT INTO users VALUES (7);"))?;
        }
        if right {
            database.execute(&sql("INSERT INTO orders VALUES (7, 10);"))?;
        }
        let mut input = source(
            &mut database,
            "SELECT * FROM users u JOIN orders o ON o.user_id = o.amount;",
        )?;
        assert_source_rows(&mut input, &mut database, &[])?;
        input.rescan();
        assert_source_rows(&mut input, &mut database, &[])?;
    }
    Ok(())
}

#[test]
fn nested_loop는_오른쪽_hash_join의_결과도_왼쪽행마다_재사용한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("nested-loop-right-hash-rescan");
    let mut database = Database::open(directory.path(), "test")?;
    seed_pairs(&mut database, false)?;
    database.execute(&sql("CREATE TABLE tags (amount BIGINT, label VARCHAR);"))?;
    for values in ["10, 'X'", "20, 'Y'"] {
        database.execute(&sql(&format!("INSERT INTO tags VALUES ({values});")))?;
    }
    let BoundStatement::Select(bound) = Binder::new(database.catalog.metadata()).bind(&sql(
        "SELECT * FROM users u JOIN orders o ON u.id = u.id JOIN tags t ON o.amount = t.amount;",
    ))?
    else {
        panic!("SELECT여야 함");
    };
    let BoundFromClause::InnerJoin {
        left,
        on: hash_condition,
        right: tags,
    } = bound.from
    else {
        panic!("바깥 조인이 있어야 함");
    };
    let BoundFromClause::InnerJoin {
        left: users,
        on: nested_condition,
        right: orders,
    } = *left
    else {
        panic!("안쪽 조인이 있어야 함");
    };
    let from = BoundFromClause::InnerJoin {
        left: users,
        on: nested_condition,
        right: Box::new(BoundFromClause::InnerJoin {
            left: orders,
            on: hash_condition,
            right: tags,
        }),
    };
    let mut input = InputState::from_clause(&mut database, &from, None)?;
    let InputState::Join(right) = &*nested_loop(&input).right else {
        panic!("오른쪽도 조인이어야 함");
    };
    assert!(right.as_any().is::<HashJoin>());
    let expected = [
        vec![
            Value::BigInt(7),
            text("A"),
            Value::BigInt(7),
            Value::BigInt(10),
            Value::BigInt(10),
            text("X"),
        ],
        vec![
            Value::BigInt(7),
            text("A"),
            Value::BigInt(7),
            Value::BigInt(20),
            Value::BigInt(20),
            text("Y"),
        ],
        vec![
            Value::BigInt(7),
            text("B"),
            Value::BigInt(7),
            Value::BigInt(10),
            Value::BigInt(10),
            text("X"),
        ],
        vec![
            Value::BigInt(7),
            text("B"),
            Value::BigInt(7),
            Value::BigInt(20),
            Value::BigInt(20),
            text("Y"),
        ],
    ];
    assert_eq!(
        next_values(&mut input, &mut database)?,
        Some(expected[0].clone())
    );
    input.rescan();
    assert_source_rows(&mut input, &mut database, &expected)?;
    input.rescan();
    assert_source_rows(&mut input, &mut database, &expected)?;
    Ok(())
}

#[test]
fn 공통입력의_nested_loop는_update와_delete에서도_대상행을_중복처리하지_않는다()
-> Result<(), DatabaseError> {
    for delete in [false, true] {
        let directory = TestDirectory::new("nested-loop-modify-input");
        let mut database = Database::open(directory.path(), "test")?;
        seed_pairs(&mut database, true)?;
        let query = if delete {
            "DELETE u FROM users u JOIN orders o ON u.id = u.id WHERE o.amount > 0;"
        } else {
            "UPDATE users u JOIN orders o ON u.id = u.id SET u.name = 'X' WHERE o.amount > 0;"
        };
        assert!(matches!(
            database.execute(&sql(query))?,
            ExecuteResult::AffectedRows { affected_rows: 2 }
        ));
        let mut cursor = cursor(&mut database, "SELECT name FROM users;")?;
        if !delete {
            assert_eq!(cursor.next_row()?, Some(vec![text("X")]));
            assert_eq!(cursor.next_row()?, Some(vec![text("X")]));
        }
        assert_eq!(cursor.next_row()?, None);
        assert_eq!(cursor.next_row()?, None);
    }
    Ok(())
}
