use std::collections::{HashMap, hash_map::Entry};

use thiserror::Error;

use crate::{
    catalog::metadata::{RelationId, TableId},
    storage::{
        StorageError, StorageManager,
        page::{PageError, Row, RowId},
    },
    table::heap::HeapTable,
};

pub mod heap;

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub enum TableError {
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct TableManager {
    tables: HashMap<TableId, HeapTable>,
}

impl TableManager {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn get_or_open_table(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
    ) -> Result<&mut HeapTable, TableError> {
        match self.tables.entry(table_id) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                storage_manager.register_relation(RelationId::Heap(table_id))?;
                Ok(entry.insert(HeapTable::new(table_id)))
            }
        }
    }

    pub fn create_table(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
    ) -> Result<(), TableError> {
        storage_manager.create_relation(RelationId::Heap(table_id))?;
        self.tables.insert(table_id, HeapTable::new(table_id));
        Ok(())
    }

    pub fn insert_row(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
        row: &Row,
    ) -> Result<RowId, TableError> {
        let heap_table = self.get_or_open_table(storage_manager, table_id)?;
        heap_table.insert(row, storage_manager)
    }

    pub fn update_row(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
        row_id: RowId,
        row: &Row,
    ) -> Result<(), TableError> {
        let heap_table = self.get_or_open_table(storage_manager, table_id)?;
        heap_table.update(storage_manager, row_id, row)
    }

    pub fn delete_row(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
        row_id: RowId,
    ) -> Result<(), TableError> {
        let heap_table = self.get_or_open_table(storage_manager, table_id)?;
        heap_table.delete(storage_manager, row_id)
    }

    pub fn scan_rows(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
    ) -> Result<Vec<(RowId, Row)>, TableError> {
        let heap_table = self.get_or_open_table(storage_manager, table_id)?;
        heap_table.scan(storage_manager)
    }

    pub fn get_rows(
        &mut self,
        storage_manager: &mut StorageManager,
        table_id: TableId,
        row_ids: Vec<RowId>,
    ) -> Result<Vec<(RowId, Row)>, TableError> {
        let heap_table = self.get_or_open_table(storage_manager, table_id)?;

        let mut rows = Vec::new();
        for row_id in row_ids {
            let row = heap_table.get(storage_manager, row_id)?;
            rows.push((row_id, row));
        }

        Ok(rows)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.tables.len()
    }
}
