use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata,
    },
    query::{
        binder::{
            BoundAssignment, BoundColumnReference, BoundDelete, BoundExpression, BoundFromClause,
            BoundOrderBy, BoundProjection, BoundSelect, BoundTable, BoundUpdate, TableInstanceId,
        },
        common::{ComparisonOperator, SortDirection},
    },
    storage::page::{PageId, RowId, SlotId},
    tuple::Value,
};

use super::{Executor, QueryRow, TableRow};

mod aggregate;
mod join;
mod select;

fn users_columns() -> Vec<ColumnMetadata> {
    vec![
        ColumnMetadata::new(ColumnId::new(1), "id".to_owned(), DataType::BigInt),
        ColumnMetadata::new(ColumnId::new(2), "name".to_owned(), DataType::Varchar),
    ]
}

fn database(table_id: TableId) -> DatabaseMetadata {
    let table = TableMetadata::new(table_id, "users".to_owned(), users_columns())
        .expect("테이블 메타데이터가 유효해야 함");
    DatabaseMetadata::new("test".to_owned(), vec![table], vec![])
        .expect("데이터베이스 메타데이터가 유효해야 함")
}

fn users_table(table_id: TableId) -> BoundTable {
    BoundTable {
        table_id,
        instance_id: TableInstanceId(0),
        alias: None,
    }
}

fn users_from(table_id: TableId) -> BoundFromClause {
    BoundFromClause::Table(users_table(table_id))
}

fn users_column(table_id: TableId, column_id: u32) -> BoundColumnReference {
    BoundColumnReference {
        table_id,
        instance_id: TableInstanceId(0),
        column_id: ColumnId::new(column_id),
    }
}

fn row(slot: u16, id: Value, name: Value) -> QueryRow {
    QueryRow {
        table_rows: vec![TableRow {
            row_id: RowId::new(PageId::new(1), SlotId::new(slot)),
            instance_id: TableInstanceId(0),
            values: vec![id, name],
        }],
    }
}

fn rows() -> Vec<QueryRow> {
    vec![
        row(1, Value::BigInt(1), Value::Varchar("Kim".to_owned())),
        row(2, Value::BigInt(2), Value::Varchar("Lee".to_owned())),
        row(3, Value::Null, Value::Varchar("Null".to_owned())),
    ]
}

fn select(table_id: TableId, projections: Vec<BoundProjection>) -> BoundSelect {
    BoundSelect {
        projections,
        from: users_from(table_id),
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    }
}

fn comparison(
    table_id: TableId,
    column_id: u32,
    operator: ComparisonOperator,
    value: Value,
) -> BoundExpression {
    BoundExpression::Comparison {
        column: users_column(table_id, column_id),
        operator,
        value,
    }
}

#[test]
fn insert는_리터럴을_row로_변환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = crate::query::binder::BoundInsert {
        table_id,
        values: vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
    };

    assert!(Executor::new(&database).encode_insert(&bound).is_ok());
}

#[test]
fn update는_filter와_일치하는_query_row만_준비한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = BoundUpdate {
        from: users_from(table_id),
        assignments: vec![BoundAssignment {
            column: users_column(table_id, 2),
            value: Value::Varchar("Park".to_owned()),
        }],
        filter: Some(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(1),
        )),
    };

    let prepared = Executor::new(&database)
        .prepare_update(rows(), &bound)
        .expect("UPDATE 대상을 준비해야 함");

    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].new_values,
        vec![Value::BigInt(1), Value::Varchar("Park".to_owned())]
    );
}

#[test]
fn delete는_filter와_일치하는_query_row만_준비한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let bound = BoundDelete {
        targets: vec![users_table(table_id)],
        from: users_from(table_id),
        filter: Some(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(2),
        )),
    };

    let prepared = Executor::new(&database)
        .prepare_delete(rows(), &bound)
        .expect("DELETE 대상을 준비해야 함");

    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].values,
        vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())]
    );
}

#[test]
fn select는_all과_column_projection을반환한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);

    assert_eq!(
        executor
            .select_rows(rows(), &select(table_id, vec![BoundProjection::All]))
            .expect("SELECT가 성공해야 함"),
        vec![
            vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())],
            vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())],
            vec![Value::Null, Value::Varchar("Null".to_owned())],
        ]
    );
    assert_eq!(
        executor
            .select_rows(
                rows(),
                &select(
                    table_id,
                    vec![BoundProjection::Column(users_column(table_id, 2))]
                ),
            )
            .expect("SELECT가 성공해야 함"),
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
            vec![Value::Varchar("Null".to_owned())],
        ]
    );
}

#[test]
fn select는_comparison_and_or_filter를적용한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let executor = Executor::new(&database);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 2))],
    );
    bound.filter = Some(BoundExpression::And {
        left: Box::new(comparison(
            table_id,
            1,
            ComparisonOperator::GreaterThan,
            Value::BigInt(0),
        )),
        right: Box::new(comparison(
            table_id,
            2,
            ComparisonOperator::Equal,
            Value::Varchar("Kim".to_owned()),
        )),
    });
    assert_eq!(
        executor
            .select_rows(rows(), &bound)
            .expect("AND filter가 성공해야 함"),
        vec![vec![Value::Varchar("Kim".to_owned())]]
    );

    bound.filter = Some(BoundExpression::Or {
        left: Box::new(comparison(
            table_id,
            1,
            ComparisonOperator::Equal,
            Value::BigInt(2),
        )),
        right: Box::new(comparison(
            table_id,
            2,
            ComparisonOperator::Equal,
            Value::Varchar("Kim".to_owned()),
        )),
    });
    assert_eq!(
        executor
            .select_rows(rows(), &bound)
            .expect("OR filter가 성공해야 함"),
        vec![
            vec![Value::Varchar("Kim".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
}

#[test]
fn select의_null_comparison은_일치하지않는다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(table_id, vec![BoundProjection::All]);
    bound.filter = Some(comparison(
        table_id,
        1,
        ComparisonOperator::Equal,
        Value::Null,
    ));

    assert!(
        Executor::new(&database)
            .select_rows(rows(), &bound)
            .expect("NULL filter가 성공해야 함")
            .is_empty()
    );
}

#[test]
fn select는_order_by와_limit을적용한다() {
    let table_id = TableId::new(1);
    let database = database(table_id);
    let mut bound = select(
        table_id,
        vec![BoundProjection::Column(users_column(table_id, 2))],
    );
    bound.order_by = Some(BoundOrderBy {
        column: users_column(table_id, 1),
        direction: SortDirection::Desc,
    });
    bound.limit = Some(2);

    assert_eq!(
        Executor::new(&database)
            .select_rows(rows(), &bound)
            .expect("ORDER BY와 LIMIT이 성공해야 함"),
        vec![
            vec![Value::Varchar("Null".to_owned())],
            vec![Value::Varchar("Lee".to_owned())],
        ]
    );
}
