use std::path::Path;

use crate::{
    binder::{BoundCreateIndex, BoundExpression, BoundSelect},
    catalog::metadata::{
        ColumnId, DatabaseMetadata, IndexId, IndexMetadata, RelationId, SchemaError, TableId,
    },
    database::ExecuteResult,
    index::btree::{BTreeKey, BTreeKeyType, tree::BTree},
    sql::ast::Literal,
    storage::{
        buffer::BufferPool,
        file::open_rw_create,
        page::{PageId, RowId},
    },
    tuple::{self, Value},
};

use super::{Database, DatabaseError};

struct IndexKey {
    index_id: IndexId,
    root_page_id: PageId,
    key: BTreeKey,
}

impl Database {
    pub(super) fn create_index(
        &mut self,
        bound: &BoundCreateIndex,
    ) -> Result<ExecuteResult, DatabaseError> {
        if self.metadata.index(&bound.index_name).is_some() {
            return Err(DatabaseError::Schema(SchemaError::DuplicateIndexName(
                bound.index_name.to_owned(),
            )));
        }

        if self.metadata.indexes().len() >= usize::from(u16::MAX) {
            return Err(DatabaseError::Schema(SchemaError::TooManyIndexes));
        }

        let table = self
            .metadata
            .table_by_id(bound.table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                bound.table_id,
            )))?;
        let column = table
            .column_by_id(bound.column_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                table_id: bound.table_id,
                column_id: bound.column_id,
            }))?;

        let table_id = table.id();
        let column_id = column.id();

        let index_id = IndexId::new(self.metadata.indexes().len() as u32 + 1);
        let key_type = BTreeKeyType::try_from(column.data_type())?;
        let btree = self.create_index_btree(index_id, key_type)?;

        let index_metadata = IndexMetadata::new(
            index_id,
            bound.index_name.clone(),
            table_id,
            column_id,
            btree.root_page_id(),
        );

        self.backfill_index(table_id, column_id, &btree)?;
        self.metadata.add_index(index_metadata)?;
        self.catalog.save(&self.metadata)?;

        Ok(ExecuteResult::Success)
    }

    pub(super) fn insert_row_into_indexes(
        &mut self,
        table_id: TableId,
        row_id: RowId,
        values: &[Value],
    ) -> Result<(), DatabaseError> {
        for index_key in self.index_keys_for_values(table_id, values)? {
            let btree = open_index_btree(
                &mut self.buffer_pool,
                index_key.index_id,
                index_key.root_page_id,
                index_key.key.key_type(),
                &self.data_dir,
            );
            btree.insert(&mut self.buffer_pool, index_key.key, row_id)?;
        }

        Ok(())
    }

    pub(super) fn delete_row_from_indexes(
        &mut self,
        table_id: TableId,
        row_id: RowId,
        values: &[Value],
    ) -> Result<(), DatabaseError> {
        for index_key in self.index_keys_for_values(table_id, values)? {
            let btree = open_index_btree(
                &mut self.buffer_pool,
                index_key.index_id,
                index_key.root_page_id,
                index_key.key.key_type(),
                &self.data_dir,
            );
            if !btree.delete(&mut self.buffer_pool, index_key.key, row_id)? {
                return Err(DatabaseError::IndexEntryNotFound {
                    index_id: index_key.index_id,
                    row_id,
                });
            }
        }

        Ok(())
    }

    fn create_index_btree(
        &mut self,
        index_id: IndexId,
        key_type: BTreeKeyType,
    ) -> Result<BTree, DatabaseError> {
        let index_path = self.data_dir.join(format!("{}.idx", index_id.id()));
        open_rw_create(&index_path)?;
        self.buffer_pool
            .register_relation(RelationId::Index(index_id), index_path);

        Ok(BTree::create(index_id, key_type, &mut self.buffer_pool)?)
    }

    fn backfill_index(
        &mut self,
        table_id: TableId,
        column_id: ColumnId,
        btree: &BTree,
    ) -> Result<(), DatabaseError> {
        let heap_table =
            self.table_cache
                .get_or_open_table(&mut self.buffer_pool, &self.data_dir, table_id)?;
        let rows = heap_table.scan(&mut self.buffer_pool)?;

        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        let index = table.column_index(column_id).ok_or(DatabaseError::Schema(
            SchemaError::IndexColumnNotFound {
                table_id: table.id(),
                column_id,
            },
        ))?;

        for (row_id, row) in rows {
            let values = tuple::decode(&row, table.columns())?;
            if let Some(key) = value_to_btree_key(values[index].clone()) {
                btree.insert(&mut self.buffer_pool, key, row_id)?;
            }
        }

        Ok(())
    }

    fn index_keys_for_values(
        &self,
        table_id: TableId,
        values: &[Value],
    ) -> Result<Vec<IndexKey>, DatabaseError> {
        let indexes = self
            .metadata
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table_id)
            .collect::<Vec<_>>();

        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        let mut index_keys = Vec::new();
        for index in indexes {
            let column = table
                .column_by_id(index.column_id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;
            let column_index = table
                .column_index(column.id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;

            if let Some(key) = value_to_btree_key(values[column_index].clone()) {
                index_keys.push(IndexKey {
                    index_id: index.id(),
                    root_page_id: index.root_page_id(),
                    key,
                });
            }
        }

        Ok(index_keys)
    }
}

pub(super) fn search_index_row_ids(
    metadata: &DatabaseMetadata,
    buffer_pool: &mut BufferPool,
    data_dir: &Path,
    bound: &BoundSelect,
) -> Result<Option<Vec<RowId>>, DatabaseError> {
    if let Some(filter) = &bound.filter {
        match filter {
            BoundExpression::Equal { column_id, value } => {
                if let Some(index_metadata) = metadata.indexes().iter().find(|index| {
                    index.table_id() == bound.table_id && index.column_id() == *column_id
                }) {
                    let table = metadata.table_by_id(index_metadata.table_id()).ok_or(
                        DatabaseError::Schema(SchemaError::IndexTableNotFound(
                            index_metadata.table_id(),
                        )),
                    )?;
                    let column = table.column_by_id(index_metadata.column_id()).ok_or(
                        DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                            table_id: index_metadata.table_id(),
                            column_id: index_metadata.column_id(),
                        }),
                    )?;

                    let key_type = BTreeKeyType::try_from(column.data_type())?;

                    let btree = open_index_btree(
                        buffer_pool,
                        index_metadata.id(),
                        index_metadata.root_page_id(),
                        key_type,
                        data_dir,
                    );

                    if let Some(key) = literal_to_btree_key(value, key_type)? {
                        return Ok(Some(btree.search_all(buffer_pool, key)?));
                    } else {
                        return Ok(Some(vec![]));
                    };
                }
            }
        }
    }

    Ok(None)
}

fn open_index_btree(
    buffer_pool: &mut BufferPool,
    index_id: IndexId,
    root_page_id: PageId,
    key_type: BTreeKeyType,
    data_dir: &Path,
) -> BTree {
    buffer_pool.register_relation(
        RelationId::Index(index_id),
        data_dir.join(format!("{}.idx", index_id.id())),
    );
    BTree::open(index_id, root_page_id, key_type)
}

fn value_to_btree_key(value: Value) -> Option<BTreeKey> {
    match value {
        Value::Int(v) => Some(BTreeKey::Int(v)),
        Value::BigInt(v) => Some(BTreeKey::BigInt(v)),
        Value::Boolean(v) => Some(BTreeKey::Boolean(v)),
        Value::Varchar(v) => Some(BTreeKey::Varchar(v)),
        Value::Null => None,
    }
}

fn literal_to_btree_key(
    literal: &Literal,
    key_type: BTreeKeyType,
) -> Result<Option<BTreeKey>, DatabaseError> {
    match (literal, key_type) {
        (Literal::Integer(v), BTreeKeyType::Int) => {
            Ok(Some(BTreeKey::Int(i32::try_from(*v).map_err(|_| {
                DatabaseError::IndexKeyOutOfRange { value: *v }
            })?)))
        }
        (Literal::Integer(v), BTreeKeyType::BigInt) => Ok(Some(BTreeKey::BigInt(*v))),
        (Literal::String(v), BTreeKeyType::Varchar) => Ok(Some(BTreeKey::Varchar(v.clone()))),
        (Literal::Null, _) => Ok(None),
        _ => Err(DatabaseError::IndexKeyTypeMismatch { key_type }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_btree_key는_null을_건너뛴다() {
        assert_eq!(value_to_btree_key(Value::Null), None);
    }
}
