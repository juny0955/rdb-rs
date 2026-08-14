use std::{
    fs::{File, OpenOptions, create_dir_all},
    io::Result,
    path::Path,
};

use crate::page::{PageId, allocate_page};

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
}

#[cfg(test)]
mod tests {
    use std::{env::temp_dir, fs};

    use super::*;

    #[test]
    fn open_테스트() -> Result<()>{
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
}
