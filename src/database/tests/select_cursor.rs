use crate::{
    catalog::metadata::TableId,
    database::{
        Database, DatabaseError,
        select_cursor::{SelectCursor, SelectSource},
    },
    query::binder::{BoundFromClause, BoundSelect, BoundTable, TableInstanceId},
    test_supports::TestDirectory,
    tuple::Value,
};

#[test]
fn buffered_cursor는_행을_소진한_뒤_계속_none을_반환한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("buffered-select-cursor");
    let mut database = Database::open(directory.path(), "test")?;
    let bound = BoundSelect {
        projections: vec![],
        from: BoundFromClause::Table(BoundTable {
            table_id: TableId::new(1),
            instance_id: TableInstanceId(0),
            alias: None,
        }),
        filter: None,
        group_by: None,
        order_by: None,
        limit: None,
    };
    let first = vec![Value::BigInt(1)];
    let second = vec![Value::BigInt(2)];
    let mut cursor = SelectCursor::new(
        &mut database,
        bound,
        SelectSource::Buffered(vec![first.clone(), second.clone()].into_iter()),
    );

    assert_eq!(cursor.next_row()?, Some(first));
    assert_eq!(cursor.next_row()?, Some(second));
    assert_eq!(cursor.next_row()?, None);
    assert_eq!(cursor.next_row()?, None);
    Ok(())
}
