use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write},
};

use super::{PAGE_SIZE, Page, PageId};

/// Database File 끝에 0으로 초기화된 8KB(PAGE_SIZE) Page를 추가하고 PageId 반환
pub fn allocate_page(file: &mut File) -> Result<PageId> {
    let file_len = file.metadata()?.len();
    if file_len % PAGE_SIZE as u64 != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "invalid database file size",
        ));
    }

    let current_page = file_len / PAGE_SIZE as u64;
    file.seek(SeekFrom::End(0))?;
    file.write_all(&Page::new().data)?;
    Ok(PageId(current_page))
}

/// Database File를 PageId의 Offset을 계산하여 8KB(PAGE_SIZE)만큼 읽는다
pub fn read_page(file: &mut File, page_id: PageId) -> Result<Page> {
    let offset = page_offset(page_id)?;
    let mut page = Page::new();

    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut page.data)?;
    Ok(page)
}

/// Database File에 PageId의 Offset을 계산하여 Page 데이터를 덮어쓴다
pub fn write_page(file: &mut File, page_id: PageId, page: &Page) -> Result<()> {
    let file_len = file.metadata()?.len();
    if file_len % PAGE_SIZE as u64 != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "invalid database file size",
        ));
    }

    if page_id.0 >= file_len / PAGE_SIZE as u64 {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid page id"));
    }

    let offset = page_offset(page_id)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&page.data)?;
    Ok(())
}

pub fn page_count(file: &File) -> Result<u64> {
    let file_len = file.metadata()?.len();
    if file_len % PAGE_SIZE as u64 != 0 {
        return Err(Error::new(ErrorKind::InvalidData, "invalid file length"));
    }

    Ok(file_len / PAGE_SIZE as u64)
}

pub(super) fn page_offset(page_id: PageId) -> Result<u64> {
    let offset = page_id
        .0
        .checked_mul(PAGE_SIZE as u64)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "page offset overflow"))?;

    Ok(offset)
}
