use crate::storage::page::error::PageError;
use crate::storage::page::{FREE_BLOCK_SIZE, PAGE_SIZE, Page, Slot, SlotId};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FreeBlock {
    pub(super) next: u16,
    pub(super) length: u16,
}

impl FreeBlock {
    pub(super) fn new(next: u16, length: u16) -> Self {
        Self { next, length }
    }

    pub(super) fn from_bytes(bytes: [u8; 4]) -> Self {
        let next = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = u16::from_be_bytes([bytes[2], bytes[3]]);

        Self { next, length }
    }

    pub(super) fn to_bytes(&self) -> [u8; 4] {
        let next = self.next.to_be_bytes();
        let length = self.length.to_be_bytes();

        [next[0], next[1], length[0], length[1]]
    }
}

impl Page {
    pub(super) fn add_free_block(&mut self, offset: u16, length: u16) -> Result<(), PageError> {
        let block = FreeBlock::new(self.free_list_head(), length);
        self.write_free_block(offset, &block)?;
        self.set_free_list_head(offset);
        Ok(())
    }

    pub(super) fn find_free_block(
        &self,
        required_len: u16,
    ) -> Result<Option<(u16, Option<u16>, FreeBlock)>, PageError> {
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

    pub(super) fn replace_free_block_link(
        &mut self,
        prev_offset: Option<u16>,
        next_offset: u16,
    ) -> Result<(), PageError> {
        if let Some(prev) = prev_offset {
            let mut prev_block = self.read_free_block(prev)?;
            prev_block.next = next_offset;
            self.write_free_block(prev, &prev_block)?;
        } else {
            self.set_free_list_head(next_offset);
        }

        Ok(())
    }

    pub(super) fn read_free_block(&self, offset: u16) -> Result<FreeBlock, PageError> {
        let offset = self.free_block_offset(offset)?;
        let mut read_bytes = [0u8; FREE_BLOCK_SIZE];
        read_bytes.copy_from_slice(&self.data[offset..offset + FREE_BLOCK_SIZE]);
        Ok(FreeBlock::from_bytes(read_bytes))
    }

    pub(super) fn write_free_block(
        &mut self,
        offset: u16,
        block: &FreeBlock,
    ) -> Result<(), PageError> {
        let offset = self.free_block_offset(offset)?;
        self.data[offset..offset + FREE_BLOCK_SIZE].copy_from_slice(&block.to_bytes());
        Ok(())
    }

    pub(super) fn free_block_offset(&self, offset: u16) -> Result<usize, PageError> {
        if offset < self.free_end()
            || offset as usize + FREE_BLOCK_SIZE > PAGE_SIZE
            || offset == u16::MAX
        {
            return Err(PageError::InvalidFreeBlockBounds);
        }

        Ok(offset as usize)
    }

    pub(super) fn try_allocate_from_free_end(
        &mut self,
        allocate_len: usize,
    ) -> Result<Option<u16>, PageError> {
        if allocate_len > self.free_space()? {
            return Ok(None);
        }

        let row_start = self.free_end() - allocate_len as u16;
        self.set_free_end(row_start);
        Ok(Some(row_start))
    }

    pub(super) fn try_allocate_from_free_block(
        &mut self,
        allocate_len: usize,
    ) -> Result<Option<u16>, PageError> {
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

    pub(super) fn compact(&mut self) -> Result<(), PageError> {
        let mut live_slots = Vec::new();
        for i in 0..self.slot_count() {
            let slot_id = SlotId(i);
            let row = match self.read_row(slot_id) {
                Ok(r) => r,
                Err(PageError::SlotNotFound) => continue,
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

    pub(super) fn free_space(&self) -> Result<usize, PageError> {
        if self.free_start() > self.free_end() {
            return Err(PageError::InvalidFreeSpaceBounds);
        }

        Ok((self.free_end() - self.free_start()) as usize)
    }
}

pub(super) fn row_allocation_size(row_len: usize) -> usize {
    let remainder = row_len % FREE_BLOCK_SIZE;
    if remainder == 0 {
        if row_len == 0 {
            return FREE_BLOCK_SIZE;
        }
        return row_len;
    }

    row_len + FREE_BLOCK_SIZE - remainder
}
