use thiserror::Error;

use crate::{
    catalog::metadata::RelationId,
    storage::manager::{StorageManager, StorageManagerError},
};
use crate::{
    catalog::metadata::TableId,
    storage::buffer::page_key::PageKey,
    storage::page::{PageError, PageId, Row, RowId},
};

#[derive(Debug, Error)]
pub enum HeapTableError {
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    StorageManager(#[from] StorageManagerError),
}

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
    ) -> Result<PageKey, HeapTableError> {
        let page_id = storage_manager.allocate_page(self.relation_id)?;
        Ok(PageKey::new(self.relation_id, page_id))
    }

    pub fn insert(
        &mut self,
        row: &Row,
        storage_manager: &mut StorageManager,
    ) -> Result<RowId, HeapTableError> {
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
                Err(e) => return Err(HeapTableError::Page(e)),
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
        row_id: RowId,
        storage_manager: &mut StorageManager,
    ) -> Result<Row, HeapTableError> {
        let frame_guard =
            storage_manager.fetch_page(PageKey::new(self.relation_id, row_id.page_id()))?;
        let page = frame_guard.page();
        let row = page.read_row(row_id.slot_id())?;

        Ok(row)
    }

    pub fn update(
        &mut self,
        row_id: RowId,
        row: &Row,
        storage_manager: &mut StorageManager,
    ) -> Result<(), HeapTableError> {
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
        row_id: RowId,
        storage_manager: &mut StorageManager,
    ) -> Result<(), HeapTableError> {
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
    ) -> Result<Vec<(RowId, Row)>, HeapTableError> {
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

#[cfg(test)]
mod tests {
    use crate::catalog::metadata::{RelationId, TableId};
    use crate::test_supports::TestRelationFile;

    use super::*;

    fn table_id() -> TableId {
        TableId::new(1)
    }

    fn storage_manager(test_file: &TestRelationFile) -> StorageManager {
        let mut storage_manager = StorageManager::new(test_file.data_dir(), 16);
        storage_manager
            .register_relation(RelationId::Heap(table_id()))
            .expect("테이블 relation을 등록해야 함");
        storage_manager
    }

    #[test]
    fn new_테스트() {
        let _ = HeapTable::new(table_id());
    }

    #[test]
    fn add_page_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("add-page", "1.tbl");
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let page_id1 = table.add_page(&mut storage_manager)?;
        let page_id2 = table.add_page(&mut storage_manager)?;
        assert_ne!(page_id1, page_id2);
        assert_eq!(storage_manager.page_count(RelationId::Heap(table_id()))?, 2);
        Ok(())
    }

    #[test]
    fn insert_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("insert", "1.tbl");
        let row = Row::from_bytes(&[1, 2, 3]);
        let row_id;
        {
            let mut table = HeapTable::new(table_id());
            let mut storage_manager = storage_manager(&test_file);
            row_id = table.insert(&row, &mut storage_manager)?;
        }
        {
            let mut table = HeapTable::new(table_id());
            let mut storage_manager = storage_manager(&test_file);
            assert_eq!(table.get(row_id, &mut storage_manager)?, row);
        }
        Ok(())
    }

    #[test]
    fn insert_storage_full_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("insert_storage_full", "1.tbl");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id1 = table.insert(&row1, &mut storage_manager)?;
        let row_id2 = table.insert(&row2, &mut storage_manager)?;

        assert_ne!(row_id1, row_id2);
        assert_eq!(storage_manager.page_count(RelationId::Heap(table_id()))?, 2);
        Ok(())
    }

    #[test]
    fn get_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("get", "1.tbl");
        let row = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id = table.insert(&row, &mut storage_manager)?;

        let read = table.get(row_id, &mut storage_manager)?;

        assert_eq!(row, read);
        Ok(())
    }

    #[test]
    fn update_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("update", "1.tbl");

        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id = table.insert(&row, &mut storage_manager)?;

        let update = Row::from_bytes(&[4, 5, 6]);
        table.update(row_id, &update, &mut storage_manager)?;
        let updated_row = table.get(row_id, &mut storage_manager)?;

        assert_ne!(row, updated_row);
        assert_eq!(update, updated_row);
        Ok(())
    }

    #[test]
    fn update_재시작_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("update-reopen", "1.tbl");
        let row = Row::from_bytes(&[1, 2, 3]);
        let updated_row = Row::from_bytes(&[4, 5, 6]);

        let row_id = {
            let mut table = HeapTable::new(table_id());
            let mut storage_manager = storage_manager(&test_file);
            let row_id = table.insert(&row, &mut storage_manager)?;
            table.update(row_id, &updated_row, &mut storage_manager)?;
            row_id
        };

        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);

        assert_eq!(table.get(row_id, &mut storage_manager)?, updated_row);
        Ok(())
    }

    #[test]
    fn delete_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("delete", "1.tbl");
        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let row_id = table.insert(&row, &mut storage_manager)?;
        let get = table.get(row_id, &mut storage_manager)?;
        assert_eq!(get, row);

        table.delete(row_id, &mut storage_manager)?;
        let error = table
            .get(row_id, &mut storage_manager)
            .expect_err("not found");
        assert!(matches!(
            error,
            HeapTableError::Page(PageError::SlotNotFound)
        ));
        Ok(())
    }

    #[test]
    fn delete_재시작_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("delete-reopen", "1.tbl");
        let row = Row::from_bytes(&[1, 2, 3]);

        let row_id = {
            let mut table = HeapTable::new(table_id());
            let mut storage_manager = storage_manager(&test_file);
            let row_id = table.insert(&row, &mut storage_manager)?;
            table.delete(row_id, &mut storage_manager)?;
            row_id
        };

        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let error = table
            .get(row_id, &mut storage_manager)
            .expect_err("삭제한 row는 재시작 뒤에도 없어야 함");

        assert!(matches!(
            error,
            HeapTableError::Page(PageError::SlotNotFound)
        ));
        Ok(())
    }

    #[test]
    fn scan_재시작_테스트() -> Result<(), HeapTableError> {
        let test_file = TestRelationFile::new("scan-reopen", "1.tbl");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[2; 200]);
        let row3 = Row::from_bytes(&[3; 200]);

        let (row_id1, row_id3) = {
            let mut table = HeapTable::new(table_id());
            let mut storage_manager = storage_manager(&test_file);
            let row_id1 = table.insert(&row1, &mut storage_manager)?;
            let row_id2 = table.insert(&row2, &mut storage_manager)?;
            let row_id3 = table.insert(&row3, &mut storage_manager)?;
            table.delete(row_id2, &mut storage_manager)?;
            (row_id1, row_id3)
        };

        let mut table = HeapTable::new(table_id());
        let mut storage_manager = storage_manager(&test_file);
        let scans = table.scan(&mut storage_manager)?;

        assert_eq!(scans, vec![(row_id1, row1), (row_id3, row3)]);
        Ok(())
    }
}
