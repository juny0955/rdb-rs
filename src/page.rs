use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write},
};

const PAGE_SIZE: usize = 8192;
const SLOT_SIZE: usize = 4;
const SLOT_COUNT_OFFSET: usize = 0;
const FREE_START_OFFSET: usize = 2;
const FREE_END_OFFSET: usize = 4;
const FREE_LIST_HEAD_OFFSET: usize = 6;
const HEADER_SIZE: usize = 8;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PageId(u64);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SlotId(u16);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RowId(PageId, SlotId);

#[derive(Debug, PartialEq, Eq)]
pub struct Slot {
    offset: u16,
    length: u16,
}

impl Slot {
    pub fn new(offset: u16, length: u16) -> Self {
        Self { offset, length }
    }

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

    pub fn tombstone(&mut self) {
        self.offset = 0;
        self.length = 0;
    }

    pub fn is_deleted(&self) -> bool {
        self.offset == 0 && self.length == 0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    data: Vec<u8>,
}

impl Row {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
        }
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.data
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
        page.set_free_list_head(u16::MAX);
        page
    }

    pub fn insert_row(&mut self, row: &Row) -> Result<SlotId> {
        let row_bytes = row.to_bytes();
        let row_len = row_bytes.len();
        if row_len > PAGE_SIZE - HEADER_SIZE - SLOT_SIZE {
            return Err(Error::new(ErrorKind::InvalidInput, "row too large"));
        }

        if SLOT_SIZE + row_len > self.free_space()? {
            return Err(Error::new(
                ErrorKind::StorageFull,
                "not enough space for row",
            ));
        }

        let row_end = self.free_end();
        let row_start = row_end - row_len as u16;

        let slot = Slot::new(row_start, row_len as u16);
        let slot_id = self.add_slot(&slot)?;

        self.data[row_start as usize..row_end as usize].copy_from_slice(row_bytes);
        self.set_free_end(row_start);

        Ok(slot_id)
    }

    pub fn read_row(&self, slot_id: SlotId) -> Result<Row> {
        let slot = self.read_slot(slot_id)?;
        let row_end = slot.offset as usize + slot.length as usize;
        if row_end > PAGE_SIZE || slot.offset < self.free_end() {
            return Err(Error::new(ErrorKind::InvalidData, "invalid row bounds"));
        }

        let row = Row::from_bytes(&self.data[slot.offset as usize..row_end]);
        Ok(row)
    }

    /// row update (같은 길이만 지원한다 추후 가변길이 지연 압축 추가 예정)
    /// 길이 다를시 InvalidInput Err
    pub fn update_row(&mut self, slot_id: SlotId, row: &Row) -> Result<()> {
        let slot = self.read_slot(slot_id)?;
        if row.to_bytes().len() != slot.length as usize {
            return Err(Error::new(ErrorKind::InvalidInput, "different row length"));
        }

        let row_end = slot.offset as usize + slot.length as usize;
        if row_end > PAGE_SIZE || slot.offset < self.free_end() {
            return Err(Error::new(ErrorKind::InvalidData, "invalid row bounds"));
        }

        self.data[slot.offset as usize..row_end].copy_from_slice(row.to_bytes());
        Ok(())
    }

    pub fn delete_row(&mut self, slot_id: SlotId) -> Result<()> {
        let mut slot = self.read_slot(slot_id)?;
        slot.tombstone();
        self.write_slot(slot_id, &slot)?;
        Ok(())
    }

    fn add_slot(&mut self, slot: &Slot) -> Result<SlotId> {
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

        let current_slot_id = SlotId(self.slot_count());
        self.write_slot(current_slot_id, slot)?;
        self.set_slot_count(current_slot_id.0 + 1);
        self.set_free_start(next_free_start);

        Ok(current_slot_id)
    }

    fn write_slot(&mut self, slot_id: SlotId, slot: &Slot) -> Result<()> {
        let offset = slot_offset(slot_id)?;
        let bytes = slot.to_bytes();

        self.data[offset..offset + SLOT_SIZE].copy_from_slice(&bytes);

        Ok(())
    }

    fn read_slot(&self, slot_id: SlotId) -> Result<Slot> {
        if slot_id.0 >= self.slot_count() {
            return Err(Error::new(ErrorKind::NotFound, "slot not found"));
        }

        let offset = slot_offset(slot_id)?;
        let mut bytes = [0u8; SLOT_SIZE];
        bytes.copy_from_slice(&self.data[offset..offset + SLOT_SIZE]);
        let slot = Slot::from_bytes(bytes);

        if slot.is_deleted() {
            return Err(Error::new(ErrorKind::NotFound, "slot not found"));
        }
        Ok(slot)
    }

    fn free_space(&self) -> Result<usize> {
        if self.free_start() > self.free_end() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "invalid free space bounds",
            ));
        }

        Ok((self.free_end() - self.free_start()) as usize)
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
        u16::from_be_bytes([self.data[FREE_END_OFFSET], self.data[FREE_LIST_HEAD_OFFSET - 1]])
    }

    pub fn set_free_end(&mut self, value: u16) {
        self.data[FREE_END_OFFSET..FREE_LIST_HEAD_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub fn free_list_head(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_LIST_HEAD_OFFSET], self.data[HEADER_SIZE - 1]])
    }

    pub fn set_free_list_head(&mut self, value: u16) {
        self.data[FREE_LIST_HEAD_OFFSET..HEADER_SIZE].copy_from_slice(&value.to_be_bytes());
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

fn slot_offset(slot_id: SlotId) -> Result<usize> {
    let offset = HEADER_SIZE + (SLOT_SIZE * slot_id.0 as usize);
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
