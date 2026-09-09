use std::path::Path;

use crate::{
    binder::{BoundCreateIndex, BoundExpression, BoundSelect},
    catalog::metadata::{DatabaseMetadata, IndexId, IndexMetadata, SchemaError, TableId},
    database::ExecuteResult,
    index::{
        btree::BTreeKeyType,
        manager::{DeleteResult, IndexManager},
    },
    storage::{manager::StorageManager, page::RowId},
    tuple::{self, Value},
};

use super::{Database, DatabaseError};

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
        let root_page_id = IndexManager::create_index_btree(
            &mut self.storage_manager,
            index_id,
            key_type,
            &self.data_dir,
        )?;

        let index_metadata = IndexMetadata::new(
            index_id,
            bound.index_name.clone(),
            table_id,
            column_id,
            root_page_id,
        );

        self.backfill_index(&index_metadata)?;
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
        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        let indexes = self
            .metadata
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table.id());

        for index_metadata in indexes {
            let column_index =
                table
                    .column_index(index_metadata.column_id())
                    .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                        table_id: table.id(),
                        column_id: index_metadata.column_id(),
                    }))?;

            let value = &values[column_index];

            IndexManager::insert_entry(
                &mut self.storage_manager,
                index_metadata,
                value,
                &self.data_dir,
                row_id,
            )?;
        }

        Ok(())
    }

    pub(super) fn delete_row_from_indexes(
        &mut self,
        table_id: TableId,
        row_id: RowId,
        values: &[Value],
    ) -> Result<(), DatabaseError> {
        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        let indexes = self
            .metadata
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table.id());

        for index_metadata in indexes {
            let column_index =
                table
                    .column_index(index_metadata.column_id())
                    .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                        table_id: table.id(),
                        column_id: index_metadata.column_id(),
                    }))?;

            let value = &values[column_index];
            match IndexManager::delete_entry(
                &mut self.storage_manager,
                index_metadata,
                value,
                &self.data_dir,
                row_id,
            )? {
                DeleteResult::Deleted | DeleteResult::SkippedNull => continue,
                DeleteResult::NotFound => {
                    return Err(DatabaseError::IndexEntryNotFound {
                        index_id: index_metadata.id(),
                        row_id,
                    });
                }
            }
        }

        Ok(())
    }

    fn backfill_index(&mut self, index_metadata: &IndexMetadata) -> Result<(), DatabaseError> {
        let heap_table = self.table_cache.get_or_open_table(
            &mut self.storage_manager,
            &self.data_dir,
            index_metadata.table_id(),
        )?;
        let rows = heap_table.scan(&mut self.storage_manager)?;

        let table =
            self.metadata
                .table_by_id(index_metadata.table_id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                    index_metadata.table_id(),
                )))?;

        let index = table
            .column_index(index_metadata.column_id())
            .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                table_id: table.id(),
                column_id: index_metadata.column_id(),
            }))?;

        let mut entries = Vec::new();
        for (row_id, row) in rows {
            let values = tuple::decode(&row, table.columns())?;
            entries.push((values[index].clone(), row_id));
        }

        IndexManager::backfill_entries(
            &mut self.storage_manager,
            index_metadata,
            entries,
            &self.data_dir,
        )?;

        Ok(())
    }
}

pub(super) fn search_index_row_ids(
    metadata: &DatabaseMetadata,
    storage_manager: &mut StorageManager,
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
                    let _ = table.column_by_id(index_metadata.column_id()).ok_or(
                        DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                            table_id: index_metadata.table_id(),
                            column_id: index_metadata.column_id(),
                        }),
                    )?;

                    return Ok(Some(IndexManager::search_row_ids(
                        storage_manager,
                        index_metadata,
                        value,
                        data_dir,
                    )?));
                }
            }
        }
    }

    Ok(None)
}
