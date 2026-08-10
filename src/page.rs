use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write},
};

const PAGE_SIZE: usize = 8192;
const SLOT_SIZE: usize = 4;
const SLOT_COUNT_OFFSET: usize = 0;
const FREE_START_OFFSET: usize = 2;
const FREE_END_OFFSET: usize = 4;
const HEADER_SIZE: usize = 6;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PageId(u64);

#[derive(Debug, PartialEq, Eq)]
pub struct Slot {
    offset: u16,
    length: u16,
}

impl Slot {
    pub fn from_bytes(bytes: [u8; SLOT_SIZE]) -> Self {
        let offset = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = u16::from_be_bytes([bytes[2], bytes[3]]);

        Self { offset, length }
    }

    pub fn to_bytes(&self) -> [u8; SLOT_SIZE] {
        let offset = self.offset.to_be_bytes();
        let length = self.length.to_be_bytes();

        [offset[0], offset[1], length[0], length[1]]
    }
}

#[derive(Debug)]
pub struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new() -> Self {
        let mut page = Self {
            data: [0u8; PAGE_SIZE],
        };

        page.set_free_start(HEADER_SIZE as u16);
        page.set_free_end(PAGE_SIZE as u16);
        page
    }

    pub fn add_slot(&mut self, slot: &Slot) -> Result<u16> {
        let next_free_start = match self.free_start().checked_add(SLOT_SIZE as u16) {
            Some(next) => {
                if next > self.free_end() {
                    return Err(Error::new(
                        ErrorKind::StorageFull,
                        "not enough space for slot",
                    ));
                }
                next
            }
            None => return Err(Error::new(ErrorKind::InvalidData, "free start overflow")),
        };

        let current_slot_index = self.slot_count();
        self.write_slot(current_slot_index, slot)?;
        self.set_slot_count(current_slot_index + 1);
        self.set_free_start(next_free_start);

        Ok(current_slot_index)
    }

    fn write_slot(&mut self, slot_index: u16, slot: &Slot) -> Result<()> {
        let offset = slot_offset(slot_index)?;
        let bytes = slot.to_bytes();

        self.data[offset..offset + SLOT_SIZE].copy_from_slice(&bytes);

        Ok(())
    }

    fn read_slot(&self, slot_index: u16) -> Result<Slot> {
        let offset = slot_offset(slot_index)?;
        let mut bytes = [0u8; SLOT_SIZE];
        bytes.copy_from_slice(&self.data[offset..offset + SLOT_SIZE]);

        Ok(Slot::from_bytes(bytes))
    }

    // getter & setter
    pub fn slot_count(&self) -> u16 {
        u16::from_be_bytes([
            self.data[SLOT_COUNT_OFFSET],
            self.data[FREE_START_OFFSET - 1],
        ])
    }

    pub fn set_slot_count(&mut self, value: u16) {
        self.data[SLOT_COUNT_OFFSET..FREE_START_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub fn free_start(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_START_OFFSET], self.data[FREE_END_OFFSET - 1]])
    }

    pub fn set_free_start(&mut self, value: u16) {
        self.data[FREE_START_OFFSET..FREE_END_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub fn free_end(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_END_OFFSET], self.data[HEADER_SIZE - 1]])
    }

    pub fn set_free_end(&mut self, value: u16) {
        self.data[FREE_END_OFFSET..HEADER_SIZE].copy_from_slice(&value.to_be_bytes());
    }
}

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

fn page_offset(page_id: PageId) -> Result<u64> {
    let offset = page_id
        .0
        .checked_mul(PAGE_SIZE as u64)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "page offset overflow"))?;

    Ok(offset)
}

fn slot_offset(slot_index: u16) -> Result<usize> {
    let offset = HEADER_SIZE + (SLOT_SIZE * slot_index as usize);
    if offset > PAGE_SIZE || offset + SLOT_SIZE > PAGE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "slot offset over page size",
        ));
    }

    Ok(offset)
}

#[cfg(test)]
mod tests;
