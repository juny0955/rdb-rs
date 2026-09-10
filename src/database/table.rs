use std::collections::{HashMap, hash_map::Entry};

use crate::{
    binder::BoundCreateTable,
    catalog::metadata::{
        ColumnId, ColumnMetadata, RelationId, SchemaError, TableId, TableMetadata,
    },
    database::{Database, DatabaseError, ExecuteResult},
    storage::{
        heap::{HeapTable, HeapTableError},
        manager::StorageManager,
    },
};

#[derive(Debug)]
pub(super) struct TableCache {
    tables: HashMap<TableId, HeapTable>,
}

impl TableCache {
    pub(super) fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub(super) fn get_or_open_table(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
    ) -> Result<&mut HeapTable, HeapTableError> {
        match self.tables.entry(table_id) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                storage_manager.register_relation(RelationId::Heap(table_id))?;
                Ok(entry.insert(HeapTable::new(table_id)))
            }
        }
    }

    fn insert(&mut self, table_id: TableId, heap_table: HeapTable) {
        self.tables.insert(table_id, heap_table);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.tables.len()
    }
}

impl Database {
    pub(super) fn create_table(
        &mut self,
        bound: &BoundCreateTable,
    ) -> Result<ExecuteResult, DatabaseError> {
        let table = self.build_table_metadata(bound)?;
        let table_id = table.id();

        self.storage_manager
            .create_relation(RelationId::Heap(table_id))
            .map_err(HeapTableError::from)?;
        let heap_table = HeapTable::new(table_id);

        self.table_cache.insert(table_id, heap_table);
        self.metadata.add_table(table)?;
        self.catalog.save(&self.metadata)?;

        Ok(ExecuteResult::Success)
    }

    fn build_table_metadata(
        &self,
        bound: &BoundCreateTable,
    ) -> Result<TableMetadata, DatabaseError> {
        if self.metadata.tables().len() >= usize::from(u16::MAX) {
            return Err(DatabaseError::Schema(SchemaError::TooManyTables));
        }
        let table_id = TableId::new(self.metadata.tables().len() as u32 + 1);

        let mut columns = Vec::new();
        for (index, column) in (1..).zip(&bound.columns) {
            let column_id = ColumnId::new(index);
            columns.push(ColumnMetadata::new(
                column_id,
                column.name.to_owned(),
                column.data_type,
            ));
        }

        let table = TableMetadata::new(table_id, bound.table.to_owned(), columns)?;
        if self.metadata.table(&bound.table).is_some() {
            return Err(DatabaseError::Schema(SchemaError::DuplicateTableName(
                bound.table.to_owned(),
            )));
        }
        Ok(table)
    }
}
