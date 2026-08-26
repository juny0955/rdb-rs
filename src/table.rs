use std::{
    fs::File,
    io::{self},
    path::Path,
};

use thiserror::Error;

use crate::{
    buffer::{BufferPool, BufferPoolError, page_key::PageKey},
    file::{open_rw, open_rw_create},
    page::{PageError, PageId, PagerError, Row, RowId, allocate_page, page_count},
    schema::TableId,
};

const BUFFER_POOL_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum HeapTableError {
    #[error("heap table I/O 오류: {0}")]
    Io(#[from] io::Error),
    #[error("page 처리 오류: {0}")]
    Page(#[from] PageError),
    #[error("pager 처리 오류: {0}")]
    Pager(#[from] PagerError),
    #[error(transparent)]
    BufferPool(#[from] BufferPoolError),
}

#[derive(Debug)]
pub struct HeapTable {
    table_id: TableId,
    file: File,
    buffer_pool: BufferPool,
}

impl HeapTable {
    pub fn open(table_id: TableId, path: &Path) -> Result<Self, HeapTableError> {
        let file = open_rw_create(path)?;
        Ok(Self {
            table_id,
            file,
            buffer_pool: BufferPool::new(BUFFER_POOL_CAPACITY),
        })
    }

    pub fn open_existing(table_id: TableId, path: &Path) -> Result<Self, HeapTableError> {
        let file = open_rw(path)?;
        Ok(Self {
            table_id,
            file,
            buffer_pool: BufferPool::new(BUFFER_POOL_CAPACITY),
        })
    }

    pub fn add_page(&mut self) -> Result<PageKey, HeapTableError> {
        let page_id = allocate_page(&mut self.file)?;
        Ok(PageKey::new(self.table_id, page_id))
    }

    pub fn insert(&mut self, row: &Row) -> Result<RowId, HeapTableError> {
        for i in 0..page_count(&self.file)? {
            let page_key = PageKey::new(self.table_id, PageId::new(i));

            let mut frame_gurad = self.buffer_pool.fetch_page(&mut self.file, page_key)?;
            let page = frame_gurad.page_mut();
            match page.insert_row(row) {
                Ok(slot_id) => {
                    frame_gurad.flush(&mut self.file)?;
                    return Ok(RowId::new(page_key.page_id(), slot_id));
                }
                Err(PageError::StorageFull) => continue,
                Err(e) => return Err(HeapTableError::Page(e)),
            };
        }

        let page_key = self.add_page()?;
        let mut frame_guard = self.buffer_pool.fetch_page(&mut self.file, page_key)?;
        let page = frame_guard.page_mut();
        let slot_id = page.insert_row(row)?;
        frame_guard.flush(&mut self.file)?;

        Ok(RowId::new(page_key.page_id(), slot_id))
    }

    pub fn get(&mut self, row_id: RowId) -> Result<Row, HeapTableError> {
        let frame_guard = self.buffer_pool.fetch_page(
            &mut self.file,
            PageKey::new(self.table_id, row_id.page_id()),
        )?;
        let page = frame_guard.page();
        let row = page.read_row(row_id.slot_id())?;

        Ok(row)
    }

    pub fn update(&mut self, row_id: RowId, row: &Row) -> Result<(), HeapTableError> {
        let mut frame_gurad = self.buffer_pool.fetch_page(
            &mut self.file,
            PageKey::new(self.table_id, row_id.page_id()),
        )?;
        let page = frame_gurad.page_mut();
        page.update_row(row_id.slot_id(), row)?;
        frame_gurad.flush(&mut self.file)?;
        Ok(())
    }

    pub fn delete(&mut self, row_id: RowId) -> Result<(), HeapTableError> {
        let mut frame_gurad = self.buffer_pool.fetch_page(
            &mut self.file,
            PageKey::new(self.table_id, row_id.page_id()),
        )?;
        let page = frame_gurad.page_mut();
        page.delete_row(row_id.slot_id())?;
        frame_gurad.flush(&mut self.file)?;
        Ok(())
    }

    pub fn scan(&mut self) -> Result<Vec<(RowId, Row)>, HeapTableError> {
        let mut scans = Vec::new();
        let page_count = page_count(&self.file)?;
        if page_count == 0 {
            return Ok(scans);
        }

        for i in 0..page_count {
            let page_key = PageKey::new(self.table_id, PageId::new(i));
            let frame_gurad = self.buffer_pool.fetch_page(&mut self.file, page_key)?;
            let page = frame_gurad.page();

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
    use crate::page::read_page;
    use crate::schema::TableId;
    use crate::test_supports::TestFile;

    use super::*;

    fn table_id() -> TableId {
        TableId::new(1)
    }

    #[test]
    fn open_테스트() -> Result<(), HeapTableError> {
        let test_file1 = TestFile::new("users1.tbl");
        let test_file2 = TestFile::new("users2.tbl");
        {
            let _ = HeapTable::open(table_id(), test_file1.path())?;
            let _ = HeapTable::open(table_id(), test_file2.path())?;
        }
        {
            let _ = HeapTable::open(table_id(), test_file1.path())?;
            let _ = HeapTable::open(table_id(), test_file2.path())?;
        }
        Ok(())
    }

    #[test]
    fn add_page_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("add-page");
        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let page_id1 = table.add_page()?;
        let page_id2 = table.add_page()?;
        assert_ne!(page_id1, page_id2);
        assert_eq!(table.file.metadata()?.len(), 8192 * 2);
        Ok(())
    }

    #[test]
    fn insert_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("insert");
        let row = Row::from_bytes(&[1, 2, 3]);
        let row_id;
        {
            let mut table = HeapTable::open(table_id(), test_file.path())?;
            row_id = table.insert(&row)?;
        }
        {
            let mut table = HeapTable::open(table_id(), test_file.path())?;
            let page = read_page(&mut table.file, row_id.page_id())?;
            let read_row = page.read_row(row_id.slot_id())?;
            assert_eq!(row, read_row);
        }
        Ok(())
    }

    #[test]
    fn insert_storage_full_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("insert_storage_full");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let row_id1 = table.insert(&row1)?;
        let row_id2 = table.insert(&row2)?;

        assert_ne!(row_id1, row_id2);
        assert_eq!(table.file.metadata()?.len(), 16384);
        Ok(())
    }

    #[test]
    fn get_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("get");
        let row = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let row_id = table.insert(&row)?;

        let read = table.get(row_id)?;

        assert_eq!(row, read);
        Ok(())
    }

    #[test]
    fn update_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("update");

        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let row_id = table.insert(&row)?;

        let update = Row::from_bytes(&[4, 5, 6]);
        table.update(row_id, &update)?;
        let updated_row = table.get(row_id)?;

        assert_ne!(row, updated_row);
        assert_eq!(update, updated_row);
        Ok(())
    }

    #[test]
    fn delete_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("delete");
        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let row_id = table.insert(&row)?;
        let get = table.get(row_id)?;
        assert_eq!(get, row);

        table.delete(row_id)?;
        let error = table.get(row_id).expect_err("not found");
        assert!(matches!(
            error,
            HeapTableError::Page(PageError::SlotNotFound)
        ));
        Ok(())
    }

    #[test]
    fn scan_재시작_테스트() -> Result<(), HeapTableError> {
        let test_file = TestFile::new("scan-reopen");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[2; 200]);
        let row3 = Row::from_bytes(&[3; 200]);

        let (row_id1, row_id3) = {
            let mut table = HeapTable::open(table_id(), test_file.path())?;
            let row_id1 = table.insert(&row1)?;
            let row_id2 = table.insert(&row2)?;
            let row_id3 = table.insert(&row3)?;
            table.delete(row_id2)?;
            (row_id1, row_id3)
        };

        let mut table = HeapTable::open(table_id(), test_file.path())?;
        let scans = table.scan()?;

        assert_eq!(scans, vec![(row_id1, row1), (row_id3, row3)]);
        Ok(())
    }
}
