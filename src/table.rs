use std::{
    fs::File,
    io::{ErrorKind, Result},
    path::Path,
};

use crate::{
    file::open_rw_create,
    page::{Page, PageId, Row, RowId, allocate_page, page_count, read_page, write_page},
};

pub struct HeapTable {
    file: File,
}

impl HeapTable {
    pub fn open(path: &Path) -> Result<Self> {
        let file = open_rw_create(path)?;
        Ok(Self { file })
    }

    pub fn add_page(&mut self) -> Result<PageId> {
        let page_id = allocate_page(&mut self.file)?;
        Ok(page_id)
    }

    pub fn insert(&mut self, row: &Row) -> Result<RowId> {
        for i in 0..page_count(&self.file)? {
            let page_id = PageId::new(i);
            let mut page = read_page(&mut self.file, page_id)?;
            match page.insert_row(row) {
                Ok(slot_id) => {
                    write_page(&mut self.file, page_id, &page)?;
                    return Ok(RowId::new(page_id, slot_id));
                }
                Err(e) if e.kind() == ErrorKind::StorageFull => continue,
                Err(e) => return Err(e),
            };
        }

        let mut page = Page::new();
        let slot_id = page.insert_row(row)?;
        let page_id = self.add_page()?;
        write_page(&mut self.file, page_id, &page)?;

        Ok(RowId::new(page_id, slot_id))
    }

    pub fn get(&mut self, row_id: RowId) -> Result<Row> {
        let page = read_page(&mut self.file, row_id.page_id())?;
        let row = page.read_row(row_id.slot_id())?;
        Ok(row)
    }

    pub fn update(&mut self, row_id: RowId, row: &Row) -> Result<()> {
        let mut page = read_page(&mut self.file, row_id.page_id())?;
        page.update_row(row_id.slot_id(), row)?;
        write_page(&mut self.file, row_id.page_id(), &page)?;
        Ok(())
    }

    pub fn delete(&mut self, row_id: RowId) -> Result<()> {
        let mut page = read_page(&mut self.file, row_id.page_id())?;
        page.delete_row(row_id.slot_id())?;
        write_page(&mut self.file, row_id.page_id(), &page)?;
        Ok(())
    }

    pub fn scan(&mut self) -> Result<Vec<(RowId, Row)>> {
        let mut scans = Vec::new();
        let page_count = page_count(&self.file)?;
        if page_count == 0 {
            return Ok(scans);
        }

        for i in 0..page_count {
            let page_id = PageId::new(i);
            let page = read_page(&mut self.file, page_id)?;

            let rows = page.scan_rows()?;
            for (slot_id, row) in rows {
                scans.push((RowId::new(page_id, slot_id), row));
            }
        }

        Ok(scans)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_supports::TestFile;

    use super::*;

    #[test]
    fn open_테스트() -> Result<()> {
        let test_file1 = TestFile::new("users1.tbl");
        let test_file2 = TestFile::new("users2.tbl");
        {
            let _ = HeapTable::open(test_file1.path())?;
            let _ = HeapTable::open(test_file2.path())?;
        }
        {
            let _ = HeapTable::open(test_file1.path())?;
            let _ = HeapTable::open(test_file2.path())?;
        }
        Ok(())
    }

    #[test]
    fn add_page_테스트() -> Result<()> {
        let test_file = TestFile::new("add-page");
        let mut table = HeapTable::open(test_file.path())?;
        let page_id1 = table.add_page()?;
        let page_id2 = table.add_page()?;
        assert_ne!(page_id1, page_id2);
        assert_eq!(table.file.metadata()?.len(), 8192 * 2);
        Ok(())
    }

    #[test]
    fn insert_테스트() -> Result<()> {
        let test_file = TestFile::new("insert");
        let row = Row::from_bytes(&[1, 2, 3]);
        let row_id;
        {
            let mut table = HeapTable::open(test_file.path())?;
            row_id = table.insert(&row)?;
        }
        {
            let mut table = HeapTable::open(test_file.path())?;
            let page = read_page(&mut table.file, row_id.page_id())?;
            let read_row = page.read_row(row_id.slot_id())?;
            assert_eq!(row, read_row);
        }
        Ok(())
    }

    #[test]
    fn insert_storage_full_테스트() -> Result<()> {
        let test_file = TestFile::new("insert_storage_full");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::open(test_file.path())?;
        let row_id1 = table.insert(&row1)?;
        let row_id2 = table.insert(&row2)?;

        assert_ne!(row_id1, row_id2);
        assert_eq!(table.file.metadata()?.len(), 16384);
        Ok(())
    }

    #[test]
    fn get_테스트() -> Result<()> {
        let test_file = TestFile::new("get");
        let row = Row::from_bytes(&[1; 200]);
        let mut table = HeapTable::open(test_file.path())?;
        let row_id = table.insert(&row)?;

        let read = table.get(row_id)?;

        assert_eq!(row, read);
        Ok(())
    }

    #[test]
    fn update_테스트() -> Result<()> {
        let test_file = TestFile::new("update");

        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::open(test_file.path())?;
        let row_id = table.insert(&row)?;

        let update = Row::from_bytes(&[4, 5, 6]);
        table.update(row_id, &update)?;
        let updated_row = table.get(row_id)?;

        assert_ne!(row, updated_row);
        assert_eq!(update, updated_row);
        Ok(())
    }

    #[test]
    fn delete_테스트() -> Result<()> {
        let test_file = TestFile::new("delete");
        let row = Row::from_bytes(&[1, 2, 3]);
        let mut table = HeapTable::open(test_file.path())?;
        let row_id = table.insert(&row)?;
        let get = table.get(row_id)?;
        assert_eq!(get, row);

        table.delete(row_id)?;
        let error = table.get(row_id).expect_err("not found");
        assert_eq!(error.kind(), ErrorKind::NotFound);
        Ok(())
    }

    #[test]
    fn scan_재시작_테스트() -> Result<()> {
        let test_file = TestFile::new("scan-reopen");
        let row1 = Row::from_bytes(&vec![1; 8000]);
        let row2 = Row::from_bytes(&[2; 200]);
        let row3 = Row::from_bytes(&[3; 200]);

        let (row_id1, row_id3) = {
            let mut table = HeapTable::open(test_file.path())?;
            let row_id1 = table.insert(&row1)?;
            let row_id2 = table.insert(&row2)?;
            let row_id3 = table.insert(&row3)?;
            table.delete(row_id2)?;
            (row_id1, row_id3)
        };

        let mut table = HeapTable::open(test_file.path())?;
        let scans = table.scan()?;

        assert_eq!(scans, vec![(row_id1, row1), (row_id3, row3)]);
        Ok(())
    }
}
