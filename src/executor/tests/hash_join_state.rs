use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata,
    },
    executor::{ExecutorError, QueryRow, TableRow},
    query::binder::{BoundColumnReference, BoundJoinCondition, TableInstanceId},
    storage::page::{PageId, RowId, SlotId},
    tuple::Value,
};

use super::HashJoinState;

fn database() -> DatabaseMetadata {
    let table = TableMetadata::new(
        TableId::new(1),
        "items".to_owned(),
        vec![
            ColumnMetadata::new(ColumnId::new(1), "id".to_owned(), DataType::BigInt),
            ColumnMetadata::new(ColumnId::new(2), "name".to_owned(), DataType::Varchar),
        ],
    )
    .expect("테이블 메타데이터가 유효해야 함");
    DatabaseMetadata::new("test".to_owned(), vec![table], vec![])
        .expect("데이터베이스 메타데이터가 유효해야 함")
}

fn column(instance: u32, id: u32) -> BoundColumnReference {
    BoundColumnReference {
        table_id: TableId::new(1),
        instance_id: TableInstanceId(instance),
        column_id: ColumnId::new(id),
    }
}

fn condition() -> BoundJoinCondition {
    BoundJoinCondition {
        left: column(0, 1),
        right: column(1, 1),
    }
}

fn row(instance: u32, slot: u16, key: Value, name: &str) -> QueryRow {
    QueryRow::new(vec![TableRow {
        row_id: RowId::new(PageId::new(u64::from(instance)), SlotId::new(slot)),
        instance_id: TableInstanceId(instance),
        values: vec![key, Value::Varchar(name.to_owned())],
    }])
}

fn assert_joined(actual: QueryRow, left: &QueryRow, right: &QueryRow) {
    assert_eq!(
        actual.table_rows.len(),
        left.table_rows.len() + right.table_rows.len()
    );
    for (actual, expected) in actual
        .table_rows
        .iter()
        .zip(left.table_rows.iter().chain(&right.table_rows))
    {
        assert_eq!(actual.instance_id, expected.instance_id);
        assert_eq!(actual.row_id, expected.row_id);
        assert_eq!(actual.values, expected.values);
    }
}

#[test]
fn hash_join_state는_중복키를_하나씩_결합하고_다른_키와_null을_제외한다() {
    let database = database();
    let condition = condition();
    let mut state = HashJoinState::new();
    let first = row(1, 10, Value::BigInt(7), "first");
    let second = row(1, 11, Value::BigInt(7), "second");
    let other = row(1, 12, Value::BigInt(9), "other");
    for right in [
        first.clone(),
        other.clone(),
        second.clone(),
        row(1, 13, Value::Null, "null"),
    ] {
        state
            .push_right(&database, right, &condition)
            .expect("오른쪽 입력을 저장해야 함");
    }
    let left = row(0, 1, Value::BigInt(7), "left");
    state
        .push_left(&database, left.clone(), &condition)
        .expect("왼쪽 키를 선택해야 함");

    assert_joined(
        state.next_joined_row().expect("첫 매칭이 있어야 함"),
        &left,
        &first,
    );
    assert_joined(
        state.next_joined_row().expect("두 번째 매칭이 있어야 함"),
        &left,
        &second,
    );
    assert!(state.next_joined_row().is_none());
    assert!(state.next_joined_row().is_none());

    let left = row(0, 2, Value::BigInt(9), "another key");
    state
        .push_left(&database, left.clone(), &condition)
        .expect("다른 키를 선택해야 함");
    assert_joined(
        state.next_joined_row().expect("키 9의 매칭이 있어야 함"),
        &left,
        &other,
    );
    assert!(state.next_joined_row().is_none());

    state
        .push_left(&database, row(0, 3, Value::Null, "null left"), &condition)
        .expect("NULL 키도 오류 없이 처리해야 함");
    assert!(state.next_joined_row().is_none());
    assert!(state.next_joined_row().is_none());
}

#[test]
fn hash_join_state는_다른_왼쪽_행에도_같은_버킷을_재사용한다() {
    let database = database();
    let condition = condition();
    let mut state = HashJoinState::new();
    let rights = [
        row(1, 10, Value::BigInt(7), "first"),
        row(1, 11, Value::BigInt(7), "second"),
    ];
    for right in &rights {
        state
            .push_right(&database, right.clone(), &condition)
            .expect("오른쪽 입력을 저장해야 함");
    }

    for left in [
        row(0, 1, Value::BigInt(7), "left first"),
        row(0, 2, Value::BigInt(7), "left second"),
    ] {
        state
            .push_left(&database, left.clone(), &condition)
            .expect("새 왼쪽 행을 설정해야 함");
        for right in &rights {
            assert_joined(
                state
                    .next_joined_row()
                    .expect("버킷의 각 행을 재사용해야 함"),
                &left,
                right,
            );
        }
        assert!(state.next_joined_row().is_none());
        assert!(state.next_joined_row().is_none());
    }
}

#[test]
fn hash_join_state는_null이나_매칭없는_왼쪽_행에서_이전_상태를_비운다() {
    let database = database();
    let condition = condition();
    let mut state = HashJoinState::new();
    for (slot, name) in [(10, "first"), (11, "second")] {
        state
            .push_right(&database, row(1, slot, Value::BigInt(7), name), &condition)
            .expect("오른쪽 입력을 저장해야 함");
    }
    state
        .push_right(&database, row(1, 12, Value::Null, "null"), &condition)
        .expect("NULL 키도 오류 없이 처리해야 함");

    for key in [Value::Null, Value::BigInt(999)] {
        state
            .push_left(
                &database,
                row(0, 1, Value::BigInt(7), "matching"),
                &condition,
            )
            .expect("기존 매칭을 설정해야 함");
        assert!(state.next_joined_row().is_some());
        state
            .push_left(&database, row(0, 2, key, "unmatched"), &condition)
            .expect("매칭 없는 행도 정상 처리해야 함");
        assert!(state.next_joined_row().is_none());
        assert!(state.next_joined_row().is_none());
    }
}

#[test]
fn hash_join_state는_새_왼쪽_행을_받으면_버킷_처음부터_시작한다() {
    let database = database();
    let condition = condition();
    let mut state = HashJoinState::new();
    let first = row(1, 10, Value::BigInt(7), "first");
    let second = row(1, 11, Value::BigInt(7), "second");
    for right in [&first, &second] {
        state
            .push_right(&database, right.clone(), &condition)
            .expect("오른쪽 입력을 저장해야 함");
    }
    state
        .push_left(
            &database,
            row(0, 1, Value::BigInt(7), "old left"),
            &condition,
        )
        .expect("이전 왼쪽 행을 설정해야 함");
    assert!(state.next_joined_row().is_some());

    let left = row(0, 2, Value::BigInt(7), "new left");
    state
        .push_left(&database, left.clone(), &condition)
        .expect("왼쪽 행을 교체해야 함");
    assert_joined(
        state.next_joined_row().expect("첫 위치로 돌아가야 함"),
        &left,
        &first,
    );
    assert_joined(
        state.next_joined_row().expect("두 번째 매칭이 있어야 함"),
        &left,
        &second,
    );
    assert!(state.next_joined_row().is_none());
}

#[test]
fn hash_join_state는_빈_상태와_빈_오른쪽_입력에서_결과가_없다() {
    let database = database();
    let mut state = HashJoinState::new();
    assert!(state.next_joined_row().is_none());
    assert!(state.next_joined_row().is_none());
    state
        .push_left(&database, row(0, 1, Value::BigInt(7), "left"), &condition())
        .expect("빈 버킷도 정상 처리해야 함");
    assert!(state.next_joined_row().is_none());
    assert!(state.next_joined_row().is_none());
}

#[test]
fn hash_join_state는_조인조건_피연산자가_반대여도_인스턴스로_키를_찾는다() {
    let database = database();
    let condition = BoundJoinCondition {
        left: column(1, 1),
        right: column(0, 1),
    };
    let mut state = HashJoinState::new();
    let right = row(1, 10, Value::BigInt(7), "right");
    let left = row(0, 1, Value::BigInt(7), "left");
    state
        .push_right(&database, right.clone(), &condition)
        .expect("물리적 오른쪽의 키를 찾아야 함");
    state
        .push_left(&database, left.clone(), &condition)
        .expect("물리적 왼쪽의 키를 찾아야 함");
    assert_joined(
        state.next_joined_row().expect("매칭이 있어야 함"),
        &left,
        &right,
    );
    assert!(state.next_joined_row().is_none());
}

#[test]
fn hash_join_state는_입력의_모든_테이블행과_식별자를_보존한다() {
    let database = database();
    let condition = condition();
    let mut state = HashJoinState::new();
    let right = row(1, 10, Value::BigInt(7), "right");
    let mut left = row(0, 1, Value::BigInt(7), "left");
    left.table_rows
        .extend(row(2, 2, Value::BigInt(99), "other table").table_rows);
    state
        .push_right(&database, right.clone(), &condition)
        .expect("오른쪽 입력을 저장해야 함");
    state
        .push_left(&database, left.clone(), &condition)
        .expect("여러 테이블을 포함한 입력을 처리해야 함");
    assert_joined(
        state.next_joined_row().expect("매칭이 있어야 함"),
        &left,
        &right,
    );
    assert!(state.next_joined_row().is_none());
}

#[test]
fn hash_join_state는_키를_찾지_못하면_오류를_반환한다() {
    let database = database();
    let mut state = HashJoinState::new();
    let condition = condition();
    assert!(matches!(
        state.push_right(
            &database,
            row(2, 10, Value::BigInt(7), "wrong instance"),
            &condition
        ),
        Err(ExecutorError::Unsupported)
    ));
    assert!(matches!(
        state.push_left(
            &database,
            row(2, 1, Value::BigInt(7), "wrong instance"),
            &condition
        ),
        Err(ExecutorError::Unsupported)
    ));

    let invalid_column = BoundJoinCondition {
        left: column(0, 999),
        right: column(1, 999),
    };
    assert!(
        matches!(state.push_right(&database, row(1, 10, Value::BigInt(7), "right"), &invalid_column), Err(ExecutorError::ColumnNotFound(id)) if id == ColumnId::new(999))
    );
    assert!(
        matches!(state.push_left(&database, row(0, 1, Value::BigInt(7), "left"), &invalid_column), Err(ExecutorError::ColumnNotFound(id)) if id == ColumnId::new(999))
    );
    assert!(state.next_joined_row().is_none());
}
