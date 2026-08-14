use std::{
    fs::{File, OpenOptions, create_dir_all},
    io::{ErrorKind, Result},
    path::Path,
};

use crate::page::{Page, PageId, Row, RowId, allocate_page, page_count, read_page, write_page};

pub struct HeapTable {
    file: File,
}

impl HeapTable {
    pub fn open(path: &Path) -> Result<Self> {
        let mut binding = OpenOptions::new();
        let options = binding.read(true).write(true).create(true);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_dir_all(parent)?;
        }

        let file = options.open(path)?;
        Ok(Self { file })
    }

    pub fn add_page(&mut self) -> Result<PageId> {
        let page_id = allocate_page(&mut self.file)?;
        Ok(page_id)
    }

    pub fn insert(&mut self, row: &Row) -> Result<RowId> {
        let mut page_id = {
            let count = page_count(&self.file)?;
            if count == 0 {
                allocate_page(&mut self.file)?
            } else {
                PageId::new(count - 1)
            }
        };

        let mut page = read_page(&mut self.file, page_id)?;
        let slot_id = match page.insert_row(row) {
            Ok(s) => s,
            Err(e) if e.kind() == ErrorKind::StorageFull => {
                page_id = allocate_page(&mut self.file)?;
                page = read_page(&mut self.file, page_id)?;
                page.insert_row(row)?
            }
            Err(e) => return Err(e),
        };
        write_page(&mut self.file, page_id, &page)?;

        Ok(RowId::new(page_id, slot_id))
    }
}

#[cfg(test)]
mod tests {
    use std::{env::temp_dir, fs};

    use super::*;

    #[test]
    fn open_테스트() -> Result<()> {
        let path1 = Path::new("users.tbl");
        let path2 = temp_dir().join("users.tbl");
        {
            let _ = HeapTable::open(&path1)?;
            let _ = HeapTable::open(&path2)?;
        }
        {
            let _ = HeapTable::open(&path1)?;
            let _ = HeapTable::open(&path2)?;
        }
        fs::remove_file(path1)?;
        fs::remove_file(path2)?;
        Ok(())
    }

    #[test]
    fn add_page_테스트() -> Result<()> {
        let path = Path::new("users1.tbl");
        let mut table = HeapTable::open(path)?;
        let page_id1 = table.add_page()?;
        let page_id2 = table.add_page()?;
        assert_ne!(page_id1, page_id2);
        assert_eq!(table.file.metadata()?.len(), 8192 * 2);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn insert_테스트() -> Result<()> {
        let path = Path::new("insert.tbl");
        let row = Row::from_bytes(&[1, 2, 3]);
        let row_id;
        {
            let mut table = HeapTable::open(path)?;
            row_id = table.insert(&row)?;
        }
        {
            let mut table = HeapTable::open(path)?;
            let page = read_page(&mut table.file, row_id.page_id())?;
            let read_row = page.read_row(row_id.slot_id())?;
            assert_eq!(row, read_row);
        }
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn insert_storage_full_테스트() -> Result<()> {
        let path = Path::new("insert_storage_full.tbl");
        {
            let row1 = Row::from_bytes(&vec![1; 8000]);
            let row2 = Row::from_bytes(&vec![1; 200]);
            let mut table = HeapTable::open(path)?;
            let row_id1 = table.insert(&row1)?;
            let row_id2 = table.insert(&row2)?;

            assert_ne!(row_id1, row_id2);
            assert_eq!(table.file.metadata()?.len(), 16384);
        }
        fs::remove_file(path)?;
        Ok(())
    }
}
