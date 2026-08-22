use std::{
    cmp::Ordering,
    io::{Error, ErrorKind, Result},
};

const PAGE_SIZE: usize = 8192;
const SLOT_SIZE: usize = 4;
const FREE_BLOCK_SIZE: usize = 4;
const SLOT_COUNT_OFFSET: usize = 0;
const FREE_START_OFFSET: usize = 2;
const FREE_END_OFFSET: usize = 4;
const FREE_LIST_HEAD_OFFSET: usize = 6;
const HEADER_SIZE: usize = 8;

mod pager;

pub(crate) use pager::{allocate_page, page_count, read_page, write_page};

#[cfg(test)]
use pager::page_offset;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PageId(u64);
impl PageId {
    pub fn new(page_id: u64) -> Self {
        PageId(page_id)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SlotId(u16);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RowId(PageId, SlotId);
impl RowId {
    pub fn new(page_id: PageId, slot_id: SlotId) -> Self {
        RowId(page_id, slot_id)
    }

    pub fn page_id(&self) -> PageId {
        self.0
    }

    pub fn slot_id(&self) -> SlotId {
        self.1
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Slot {
    offset: u16,
    length: u16,
}

impl Slot {
    fn new(offset: u16, length: u16) -> Self {
        Self { offset, length }
    }

    fn from_bytes(bytes: [u8; SLOT_SIZE]) -> Self {
        let offset = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = u16::from_be_bytes([bytes[2], bytes[3]]);

        Self { offset, length }
    }

    fn to_bytes(&self) -> [u8; SLOT_SIZE] {
        let offset = self.offset.to_be_bytes();
        let length = self.length.to_be_bytes();

        [offset[0], offset[1], length[0], length[1]]
    }

    fn tombstone(&mut self) {
        self.offset = 0;
        self.length = 0;
    }

    fn is_deleted(&self) -> bool {
        self.offset == 0 && self.length == 0
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FreeBlock {
    next: u16,
    length: u16,
}

impl FreeBlock {
    fn new(next: u16, length: u16) -> Self {
        Self { next, length }
    }

    fn from_bytes(bytes: [u8; 4]) -> Self {
        let next = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = u16::from_be_bytes([bytes[2], bytes[3]]);

        Self { next, length }
    }

    fn to_bytes(&self) -> [u8; 4] {
        let next = self.next.to_be_bytes();
        let length = self.length.to_be_bytes();

        [next[0], next[1], length[0], length[1]]
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
        let allocate_len = row_allocation_size(row_len);

        if allocate_len > PAGE_SIZE - HEADER_SIZE - SLOT_SIZE {
            return Err(Error::new(ErrorKind::InvalidInput, "row too large"));
        }

        if let Some(slot_id) = self.try_insert_from_free_block(row_bytes, allocate_len)? {
            return Ok(slot_id);
        }

        match self.insert_from_free_end(row_bytes, allocate_len) {
            Ok(slot_id) => Ok(slot_id),
            Err(e) if e.kind() == ErrorKind::StorageFull => {
                self.compact()?;
                Ok(self.insert_from_free_end(row_bytes, allocate_len)?)
            }
            Err(e) => Err(e),
        }
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

    pub fn update_row(&mut self, slot_id: SlotId, row: &Row) -> Result<()> {
        let mut slot = self.read_slot(slot_id)?;
        let slot_length = slot.length as usize;
        let slot_offset = slot.offset as usize;
        let row_bytes = row.to_bytes();
        let mut old_allocate_len = row_allocation_size(slot_length);
        let new_allocate_len = row_allocation_size(row_bytes.len());
        let allocate_end = slot_offset + old_allocate_len;

        if allocate_end > PAGE_SIZE || slot_offset < self.free_end() as usize {
            return Err(Error::new(ErrorKind::InvalidData, "invalid row bounds"));
        }

        match new_allocate_len.cmp(&old_allocate_len) {
            Ordering::Equal => {
                let new_row_end = slot_offset + row_bytes.len();

                slot.length = row_bytes.len() as u16;
                self.write_slot(slot_id, &slot)?;

                self.data[slot_offset..new_row_end].copy_from_slice(row_bytes);
            }
            Ordering::Less => {
                self.write_row_bytes_at(slot.offset, row_bytes)?;

                slot.length = row_bytes.len() as u16;
                self.write_slot(slot_id, &slot)?;

                let free_block_length = (old_allocate_len - new_allocate_len) as u16;
                let free_block_offset = (slot_offset + new_allocate_len) as u16;
                self.add_free_block(free_block_offset, free_block_length)?;
            }
            Ordering::Greater => {
                let new_offset = match self.try_allocate_from_free_block(new_allocate_len)? {
                    Some(offset) => Some(offset),
                    None => match self.try_allocate_from_free_end(new_allocate_len)? {
                        Some(offset) => Some(offset),
                        None => {
                            self.compact()?;
                            slot = self.read_slot(slot_id)?;
                            old_allocate_len = row_allocation_size(slot.length as usize);
                            self.try_allocate_from_free_end(new_allocate_len)?
                        }
                    },
                }
                .ok_or_else(|| Error::new(ErrorKind::StorageFull, "not enough space for row"))?;

                self.write_row_bytes_at(new_offset, row_bytes)?;

                let old_slot_offset = slot.offset;
                slot.offset = new_offset;
                slot.length = row_bytes.len() as u16;
                self.write_slot(slot_id, &slot)?;

                self.add_free_block(old_slot_offset, old_allocate_len as u16)?;
            }
        }

        Ok(())
    }

    pub fn delete_row(&mut self, slot_id: SlotId) -> Result<()> {
        let mut slot = self.read_slot(slot_id)?;
        let row_offset = slot.offset;
        let allocate_len = row_allocation_size(slot.length as usize);

        self.add_free_block(row_offset, allocate_len as u16)?;

        slot.tombstone();
        self.write_slot(slot_id, &slot)?;
        Ok(())
    }

    pub fn scan_rows(&self) -> Result<Vec<(SlotId, Row)>> {
        let mut scans = Vec::new();

        let slot_count = self.slot_count();
        for i in 0..slot_count {
            let slot_id = SlotId(i);
            match self.read_row(slot_id) {
                Ok(row) => scans.push((slot_id, row)),
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(scans)
    }

    fn try_insert_from_free_block(
        &mut self,
        row_bytes: &[u8],
        allocate_len: usize,
    ) -> Result<Option<SlotId>> {
        if SLOT_SIZE > self.free_space()? {
            return Ok(None);
        }

        self.try_allocate_from_free_block(allocate_len)?
            .map(|offset| self.write_row_at(offset, row_bytes))
            .transpose()
    }

    fn try_allocate_from_free_end(&mut self, allocate_len: usize) -> Result<Option<u16>> {
        if allocate_len > self.free_space()? {
            return Ok(None);
        }

        let row_start = self.free_end() - allocate_len as u16;
        self.set_free_end(row_start);
        Ok(Some(row_start))
    }

    fn try_allocate_from_free_block(&mut self, allocate_len: usize) -> Result<Option<u16>> {
        if let Some((current_offset, prev_offset, block)) =
            self.find_free_block(allocate_len as u16)?
        {
            let block_len = block.length as usize;
            let block_offset = current_offset as usize;

            if block_len > allocate_len {
                let remaining_length = (block_len - allocate_len) as u16;
                let remaining_offset = (block_offset + allocate_len) as u16;
                let remaining_block = FreeBlock::new(block.next, remaining_length);

                self.write_free_block(remaining_offset, &remaining_block)?;
                self.replace_free_block_link(prev_offset, remaining_offset)?;
            } else if block_len == allocate_len {
                self.replace_free_block_link(prev_offset, block.next)?;
            }

            return Ok(Some(current_offset));
        }

        Ok(None)
    }

    fn insert_from_free_end(&mut self, row_bytes: &[u8], allocate_len: usize) -> Result<SlotId> {
        if SLOT_SIZE + allocate_len > self.free_space()? {
            return Err(Error::new(
                ErrorKind::StorageFull,
                "not enough space for row",
            ));
        }

        let row_end = self.free_end();
        let row_start = row_end - allocate_len as u16;

        let slot_id = self.write_row_at(row_start, row_bytes)?;
        self.set_free_end(row_start);

        Ok(slot_id)
    }

    fn write_row_bytes_at(&mut self, offset: u16, row_bytes: &[u8]) -> Result<()> {
        let offset = offset as usize;
        self.data[offset..offset + row_bytes.len()].copy_from_slice(row_bytes);
        Ok(())
    }

    fn write_row_at(&mut self, offset: u16, row_bytes: &[u8]) -> Result<SlotId> {
        let slot = Slot::new(offset, row_bytes.len() as u16);
        let slot_id = self.add_slot(&slot)?;

        let offset = offset as usize;
        self.data[offset..offset + row_bytes.len()].copy_from_slice(row_bytes);
        Ok(slot_id)
    }

    fn compact(&mut self) -> Result<()> {
        let mut live_slots = Vec::new();
        for i in 0..self.slot_count() {
            let slot_id = SlotId(i);
            let row = match self.read_row(slot_id) {
                Ok(r) => r,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };

            live_slots.push((slot_id, row.to_bytes().to_vec()));
        }

        self.set_free_list_head(u16::MAX);
        self.set_free_end(PAGE_SIZE as u16);

        for (slot_id, row_bytes) in live_slots {
            let allocation_len = row_allocation_size(row_bytes.len());

            let new_offset = self.free_end() as usize - allocation_len;
            self.data[new_offset..new_offset + row_bytes.len()].copy_from_slice(&row_bytes);

            let new_offset = new_offset as u16;
            let slot = Slot::new(new_offset, row_bytes.len() as u16);
            self.write_slot(slot_id, &slot)?;
            self.set_free_end(new_offset);
        }

        Ok(())
    }

    fn find_free_block(&self, required_len: u16) -> Result<Option<(u16, Option<u16>, FreeBlock)>> {
        let mut current_offset = self.free_list_head();
        let mut prev_offset = None;

        while current_offset != u16::MAX {
            let block = self.read_free_block(current_offset)?;
            if block.length >= required_len {
                return Ok(Some((current_offset, prev_offset, block)));
            }

            prev_offset = Some(current_offset);
            current_offset = block.next;
        }

        Ok(None)
    }

    fn replace_free_block_link(
        &mut self,
        prev_offset: Option<u16>,
        next_offset: u16,
    ) -> Result<()> {
        if let Some(prev) = prev_offset {
            let mut prev_block = self.read_free_block(prev)?;
            prev_block.next = next_offset;
            self.write_free_block(prev, &prev_block)?;
        } else {
            self.set_free_list_head(next_offset);
        }

        Ok(())
    }

    fn read_free_block(&self, offset: u16) -> Result<FreeBlock> {
        let offset = self.free_block_offset(offset)?;
        let mut read_bytes = [0u8; FREE_BLOCK_SIZE];
        read_bytes.copy_from_slice(&self.data[offset..offset + FREE_BLOCK_SIZE]);
        Ok(FreeBlock::from_bytes(read_bytes))
    }

    fn write_free_block(&mut self, offset: u16, block: &FreeBlock) -> Result<()> {
        let offset = self.free_block_offset(offset)?;
        self.data[offset..offset + FREE_BLOCK_SIZE].copy_from_slice(&block.to_bytes());
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

    fn write_slot(&mut self, slot_id: SlotId, slot: &Slot) -> Result<()> {
        let offset = slot_offset(slot_id)?;
        let bytes = slot.to_bytes();

        self.data[offset..offset + SLOT_SIZE].copy_from_slice(&bytes);

        Ok(())
    }

    fn add_free_block(&mut self, offset: u16, length: u16) -> Result<()> {
        let block = FreeBlock::new(self.free_list_head(), length);
        self.write_free_block(offset, &block)?;
        self.set_free_list_head(offset);
        Ok(())
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

    fn free_block_offset(&self, offset: u16) -> Result<usize> {
        if offset < self.free_end()
            || offset as usize + FREE_BLOCK_SIZE > PAGE_SIZE
            || offset == u16::MAX
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "invalid free block bounds",
            ));
        }

        Ok(offset as usize)
    }

    // getter & setter
    fn slot_count(&self) -> u16 {
        u16::from_be_bytes([
            self.data[SLOT_COUNT_OFFSET],
            self.data[FREE_START_OFFSET - 1],
        ])
    }

    fn set_slot_count(&mut self, value: u16) {
        self.data[SLOT_COUNT_OFFSET..FREE_START_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    fn free_start(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_START_OFFSET], self.data[FREE_END_OFFSET - 1]])
    }

    fn set_free_start(&mut self, value: u16) {
        self.data[FREE_START_OFFSET..FREE_END_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    fn free_end(&self) -> u16 {
        u16::from_be_bytes([
            self.data[FREE_END_OFFSET],
            self.data[FREE_LIST_HEAD_OFFSET - 1],
        ])
    }

    fn set_free_end(&mut self, value: u16) {
        self.data[FREE_END_OFFSET..FREE_LIST_HEAD_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    fn free_list_head(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_LIST_HEAD_OFFSET], self.data[HEADER_SIZE - 1]])
    }

    fn set_free_list_head(&mut self, value: u16) {
        self.data[FREE_LIST_HEAD_OFFSET..HEADER_SIZE].copy_from_slice(&value.to_be_bytes());
    }
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

fn row_allocation_size(row_len: usize) -> usize {
    let remainder = row_len % FREE_BLOCK_SIZE;
    if remainder == 0 {
        if row_len == 0 {
            return FREE_BLOCK_SIZE;
        }
        return row_len;
    }

    row_len + FREE_BLOCK_SIZE - remainder
}

#[cfg(test)]
mod tests;
