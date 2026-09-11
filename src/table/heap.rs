use crate::{catalog::metadata::RelationId, storage::StorageManager, table::TableError};
use crate::{
    catalog::metadata::TableId,
    storage::buffer::page_key::PageKey,
    storage::page::{PageError, PageId, Row, RowId},
};

#[derive(Debug)]
pub struct HeapTable {
    relation_id: RelationId,
}

impl HeapTable {
    pub fn new(table_id: TableId) -> Self {
        Self {
            relation_id: RelationId::Heap(table_id),
        }
    }

    pub fn add_page(
        &mut self,
        storage_manager: &mut StorageManager,
    ) -> Result<PageKey, TableError> {
        let page_id = storage_manager.allocate_page(self.relation_id)?;
        Ok(PageKey::new(self.relation_id, page_id))
    }

    pub fn insert(
        &mut self,
        row: &Row,
        storage_manager: &mut StorageManager,
    ) -> Result<RowId, TableError> {
        for i in 0..storage_manager.page_count(self.relation_id)? {
            let page_key = PageKey::new(self.relation_id, PageId::new(i));

            let result = {
                let mut frame_guard = storage_manager.fetch_page(page_key)?;
                let page = frame_guard.page_mut();
                page.insert_row(row)
            };

            match result {
                Ok(slot_id) => {
                    storage_manager.flush_page(page_key)?;
                    return Ok(RowId::new(page_key.page_id(), slot_id));
                }
                Err(PageError::StorageFull) => continue,
                Err(e) => return Err(TableError::Page(e)),
            };
        }

        let page_key = self.add_page(storage_manager)?;
        let slot_id = {
            let mut frame_guard = storage_manager.fetch_page(page_key)?;
            let page = frame_guard.page_mut();
            page.insert_row(row)?
        };
        storage_manager.flush_page(page_key)?;

        Ok(RowId::new(page_key.page_id(), slot_id))
    }

    pub fn get(
        &mut self,
        storage_manager: &mut StorageManager,
        row_id: RowId,
    ) -> Result<Row, TableError> {
        let frame_guard =
            storage_manager.fetch_page(PageKey::new(self.relation_id, row_id.page_id()))?;
        let page = frame_guard.page();
        let row = page.read_row(row_id.slot_id())?;

        Ok(row)
    }

    pub fn update(
        &mut self,
        storage_manager: &mut StorageManager,
        row_id: RowId,
        row: &Row,
    ) -> Result<(), TableError> {
        {
            let mut frame_guard =
                storage_manager.fetch_page(PageKey::new(self.relation_id, row_id.page_id()))?;
            let page = frame_guard.page_mut();
            page.update_row(row_id.slot_id(), row)?;
        }
        storage_manager.flush_page(PageKey::new(self.relation_id, row_id.page_id()))?;
        Ok(())
    }

    pub fn delete(
        &mut self,
        storage_manager: &mut StorageManager,
        row_id: RowId,
    ) -> Result<(), TableError> {
        let page_key = PageKey::new(self.relation_id, row_id.page_id());
        {
            let mut frame_guard = storage_manager.fetch_page(page_key)?;
            let page = frame_guard.page_mut();
            page.delete_row(row_id.slot_id())?;
        }
        storage_manager.flush_page(page_key)?;
        Ok(())
    }

    pub fn scan(
        &mut self,
        storage_manager: &mut StorageManager,
    ) -> Result<Vec<(RowId, Row)>, TableError> {
        let mut scans = Vec::new();
        let page_count = storage_manager.page_count(self.relation_id)?;
        if page_count == 0 {
            return Ok(scans);
        }

        for i in 0..page_count {
            let page_key = PageKey::new(self.relation_id, PageId::new(i));
            let frame_guard = storage_manager.fetch_page(page_key)?;
            let page = frame_guard.page();

            let rows = page.scan_rows()?;
            for (slot_id, row) in rows {
                scans.push((RowId::new(page_key.page_id(), slot_id), row));
            }
        }

        Ok(scans)
    }
}
