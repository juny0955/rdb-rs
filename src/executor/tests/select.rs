use crate::{
    catalog::metadata::{ColumnId, TableId},
    executor::{
        ExecutorError,
        select::{SelectState, SelectStep},
    },
    query::{
        binder::{BoundAggregate, BoundOrderBy, BoundProjection},
        common::{ComparisonOperator, SortDirection},
    },
    tuple::Value,
};

use super::{Executor, QueryRow, comparison, database, row, select, users_column};

#[test]
fn select_state는_limit_없이_여러_행을_projection_순서로_반환하고_eof에서_종료한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Column(users_column(table_id, 2)),
            BoundProjection::Column(users_column(table_id, 1)),
        ],
    );
    let mut state = SelectState::new(bound).expect("SELECT 계산 상태를 생성해야 함");

    for (slot, id, name) in [(1, Value::BigInt(10), "Kim"), (2, Value::Null, "Lee")] {
        let name = Value::Varchar(name.to_owned());
        let expected = vec![name.clone(), id.clone()];
        let input = row(slot, id, name);
        match state
            .push_row(&executor, &input)
            .expect("결과 행을 계산해야 함")
        {
            SelectStep::Row(values) => assert_eq!(values, expected),
            _ => panic!("EOF 이전에는 입력마다 결과 행을 반환해야 함"),
        }
        assert!(!state.is_done());
    }

    for _ in 0..2 {
        assert!(matches!(
            state.finish().expect("일반 SELECT의 EOF를 처리해야 함"),
            SelectStep::Done
        ));
        assert!(state.is_done());
    }
    assert!(matches!(
        state
            .push_row(&executor, &QueryRow::new(vec![]))
            .expect("EOF 후에는 입력을 계산하면 안 됨"),
        SelectStep::Done
    ));
}

#[test]
fn aggregate_state는_null_행을_count에_포함하고_sum은_null로_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );
    let mut state = SelectState::new(bound).expect("집계 계산 상태를 생성해야 함");

    for slot in [1, 2] {
        let input = row(slot, Value::Null, Value::Null);
        assert!(matches!(
            state
                .push_row(&executor, &input)
                .expect("NULL 행을 누적해야 함"),
            SelectStep::NeedInput
        ));
        assert!(!state.is_done());
    }
    match state.finish().expect("NULL 입력의 집계 결과를 확정해야 함") {
        SelectStep::Row(values) => assert_eq!(values, vec![Value::BigInt(2), Value::Null]),
        _ => panic!("NULL 행도 COUNT에 포함해야 함"),
    }
    assert!(state.is_done());
}

#[test]
fn aggregate_state는_여러_누산기를_독립적으로_관리하고_projection_순서를_유지한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );
    let mut state = SelectState::new(bound).expect("여러 누산기를 생성해야 함");
    for (slot, value) in [
        (1, Value::BigInt(10)),
        (2, Value::Null),
        (3, Value::BigInt(3)),
    ] {
        let input = row(slot, value, Value::Null);
        assert!(matches!(
            state
                .push_row(&executor, &input)
                .expect("각 누산기에 입력을 반영해야 함"),
            SelectStep::NeedInput
        ));
    }
    match state.finish().expect("여러 집계 결과를 확정해야 함") {
        SelectStep::Row(values) => {
            assert_eq!(
                values,
                vec![Value::BigInt(13), Value::BigInt(3), Value::BigInt(13)]
            );
        }
        _ => panic!("SUM, COUNT, SUM 순서로 결과를 반환해야 함"),
    }
    assert!(state.is_done());
}

#[test]
fn aggregate_state는_sum의_양수와_음수_overflow를_전파한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);

    for (first, second) in [(i64::MAX, 1), (i64::MIN, -1)] {
        let bound = select(
            table_id,
            vec![BoundProjection::Aggregate(BoundAggregate::Sum(
                users_column(table_id, 1),
            ))],
        );
        let mut state = SelectState::new(bound).expect("SUM 계산 상태를 생성해야 함");
        let first = row(1, Value::BigInt(first), Value::Null);
        assert!(matches!(
            state
                .push_row(&executor, &first)
                .expect("첫 입력은 BIGINT 범위 안이어야 함"),
            SelectStep::NeedInput
        ));
        let second = row(2, Value::BigInt(second), Value::Null);
        assert!(matches!(
            state.push_row(&executor, &second),
            Err(ExecutorError::SumOverflow)
        ));
    }
}

#[test]
fn select_state는_group_by_order_by와_일반_집계_projection_혼합을_거부한다() {
    let table_id = TableId::new(1);
    let mut grouped = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 1))],
    );
    grouped.group_by = Some(vec![users_column(table_id, 1)]);
    let mut ordered = select(table_id, vec![BoundProjection::All]);
    ordered.order_by = Some(BoundOrderBy {
        column: users_column(table_id, 1),
        direction: SortDirection::Asc,
    });
    let column_first = select(
        table_id,
        vec![
            BoundProjection::Column(users_column(table_id, 1)),
            BoundProjection::Aggregate(BoundAggregate::CountAll),
        ],
    );
    let aggregate_first = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Column(users_column(table_id, 1)),
        ],
    );
    for bound in [grouped, ordered, column_first, aggregate_first] {
        assert!(matches!(
            SelectState::new(bound),
            Err(ExecutorError::Unsupported)
        ));
    }
}

#[test]
fn select_state는_filter와_projection의_컬럼_오류를_전파한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let input = row(1, Value::BigInt(1), Value::Null);
    let mut filtered = select(table_id, vec![BoundProjection::All]);
    filtered.filter = Some(comparison(
        table_id,
        999,
        ComparisonOperator::Equal,
        Value::BigInt(1),
    ));
    let projected = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 999))],
    );
    for bound in [filtered, projected] {
        let mut state = SelectState::new(bound).expect("SELECT 계산 상태를 생성해야 함");
        assert!(matches!(
            state.push_row(&executor, &input),
            Err(ExecutorError::ColumnNotFound(column)) if column == ColumnId::new(999)
        ));
    }
}

#[test]
fn aggregate_state는_where를_통과한_행만_count와_sum에_반영한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);

    for (threshold, expected) in [
        (2, vec![Value::BigInt(2), Value::BigInt(13)]),
        (100, vec![Value::BigInt(0), Value::Null]),
    ] {
        let mut bound = select(
            table_id,
            vec![
                BoundProjection::Aggregate(BoundAggregate::CountAll),
                BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
            ],
        );
        bound.filter = Some(comparison(
            table_id,
            1,
            ComparisonOperator::GreaterThan,
            Value::BigInt(threshold),
        ));
        let mut state = SelectState::new(bound).expect("필터가 있는 집계 상태를 생성해야 함");

        for (slot, value) in [
            (1, Value::BigInt(1)),
            (2, Value::BigInt(10)),
            (3, Value::Null),
            (4, Value::BigInt(3)),
        ] {
            let input = row(slot, value, Value::Null);
            assert!(matches!(
                state
                    .push_row(&executor, &input)
                    .expect("집계 전에 WHERE를 적용해야 함"),
                SelectStep::NeedInput
            ));
            assert!(!state.is_done());
        }

        match state
            .finish()
            .expect("필터 적용 후 집계 결과를 확정해야 함")
        {
            SelectStep::Row(values) => assert_eq!(values, expected),
            _ => panic!("모든 행이 탈락해도 집계 결과 한 행을 반환해야 함"),
        }
        assert!(state.is_done());
        assert!(matches!(
            state.finish().expect("집계 결과가 다시 생성되면 안 됨"),
            SelectStep::Done
        ));
    }
}

#[test]
fn limit_zero는_일반과_집계_모드에서_입력_계산_없이_종료한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let input = QueryRow::new(vec![]);

    for projections in [
        vec![BoundProjection::Column(users_column(table_id, 1))],
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    ] {
        let mut bound = select(table_id, projections);
        bound.filter = Some(comparison(
            table_id,
            1,
            ComparisonOperator::GreaterThan,
            Value::BigInt(0),
        ));
        bound.limit = Some(0);
        let mut state = SelectState::new(bound).expect("LIMIT 0 계산 상태를 생성해야 함");
        assert!(state.is_done());

        for _ in 0..2 {
            assert!(matches!(
                state
                    .push_row(&executor, &input)
                    .expect("LIMIT 0은 컬럼이 없는 입력도 계산하지 않아야 함"),
                SelectStep::Done
            ));
            assert!(matches!(
                state.finish().expect("LIMIT 0은 결과를 출력하면 안 됨"),
                SelectStep::Done
            ));
            assert!(state.is_done());
        }
    }
}

#[test]
fn select_state는_빈_입력을_결과_없이_종료한다() {
    let table_id = TableId::new(1);
    let bound = select(table_id, vec![BoundProjection::All]);
    let mut state = SelectState::new(bound).expect("SELECT 계산 상태를 생성해야 함");
    assert!(!state.is_done());

    assert!(matches!(
        state.finish().expect("빈 입력의 SELECT를 마무리해야 함"),
        SelectStep::Done
    ));
    assert!(state.is_done());
    assert!(matches!(
        state.finish().expect("반복 마무리는 Done을 반환해야 함"),
        SelectStep::Done
    ));
}

#[test]
fn aggregate_state는_빈_입력에서_count_0과_sum_null을_한번만_반환한다() {
    let table_id = TableId::new(1);
    let bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );
    let mut state = SelectState::new(bound).expect("집계 계산 상태를 생성해야 함");
    assert!(!state.is_done());

    match state.finish().expect("빈 입력의 집계 결과를 확정해야 함") {
        SelectStep::Row(values) => assert_eq!(values, vec![Value::BigInt(0), Value::Null]),
        _ => panic!("빈 입력이어도 COUNT와 SUM 결과 한 행을 반환해야 함"),
    }
    assert!(state.is_done());
    assert!(matches!(
        state.finish().expect("집계 결과가 다시 생성되면 안 됨"),
        SelectStep::Done
    ));
}

#[test]
fn aggregate_limit_one은_전체_입력을_누적하고_결과를_한번만_반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut bound = select(
        table_id,
        vec![
            BoundProjection::Aggregate(BoundAggregate::CountAll),
            BoundProjection::Aggregate(BoundAggregate::Sum(users_column(table_id, 1))),
        ],
    );
    bound.limit = Some(1);
    let mut state = SelectState::new(bound).expect("집계 계산 상태를 생성해야 함");
    assert!(!state.is_done());

    for (slot, value) in [
        (1, Value::BigInt(10)),
        (2, Value::Null),
        (3, Value::BigInt(3)),
    ] {
        let input = row(slot, value, Value::Null);
        assert!(matches!(
            state
                .push_row(&executor, &input)
                .expect("집계 입력을 누적해야 함"),
            SelectStep::NeedInput
        ));
        assert!(!state.is_done());
    }

    match state.finish().expect("집계 결과를 확정해야 함") {
        SelectStep::Row(values) => {
            assert_eq!(values, vec![Value::BigInt(3), Value::BigInt(13)]);
        }
        _ => panic!("전체 입력의 COUNT와 SUM 결과를 반환해야 함"),
    }
    assert!(state.is_done());

    assert!(matches!(
        state.finish().expect("반복 마무리는 Done을 반환해야 함"),
        SelectStep::Done
    ));
    let input = row(4, Value::BigInt(100), Value::Null);
    assert!(matches!(
        state
            .push_row(&executor, &input)
            .expect("집계 종료 후 입력은 Done을 반환해야 함"),
        SelectStep::Done
    ));
    assert!(matches!(
        state.finish().expect("집계 결과가 다시 생성되면 안 됨"),
        SelectStep::Done
    ));
}

#[test]
fn select_state는_필터_탈락과_limit_종료를_구분한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 1))],
    );
    bound.filter = Some(comparison(
        table_id,
        1,
        ComparisonOperator::GreaterThan,
        Value::BigInt(2),
    ));
    bound.limit = Some(1);
    let mut state = SelectState::new(bound).expect("SELECT 계산 상태를 생성해야 함");
    assert!(!state.is_done());

    for (slot, id) in [(1, 1), (2, 2)] {
        let input = row(slot, Value::BigInt(id), Value::Null);
        assert!(matches!(
            state
                .push_row(&executor, &input)
                .expect("필터 탈락 행을 처리해야 함"),
            SelectStep::NeedInput
        ));
        assert!(!state.is_done());
    }

    let input = row(3, Value::BigInt(3), Value::Null);
    match state
        .push_row(&executor, &input)
        .expect("필터에 일치하는 행을 처리해야 함")
    {
        SelectStep::Row(values) => assert_eq!(values, vec![Value::BigInt(3)]),
        _ => panic!("LIMIT에 도달한 마지막 결과 행을 반환해야 함"),
    }
    assert!(state.is_done());

    let input = row(4, Value::BigInt(4), Value::Null);
    assert!(matches!(
        state
            .push_row(&executor, &input)
            .expect("종료 후 입력은 Done을 반환해야 함"),
        SelectStep::Done
    ));
    assert!(state.is_done());
    assert!(matches!(
        state.finish().expect("종료된 SELECT를 마무리해야 함"),
        SelectStep::Done
    ));
}
