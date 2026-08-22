use std::io::{Error, ErrorKind, Result};

use crate::page::{FREE_BLOCK_SIZE, PAGE_SIZE, Page};

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
    pub(super) fn add_free_block(&mut self, offset: u16, length: u16) -> Result<()> {
        let block = FreeBlock::new(self.free_list_head(), length);
        self.write_free_block(offset, &block)?;
        self.set_free_list_head(offset);
        Ok(())
    }

    pub(super) fn find_free_block(
        &self,
        required_len: u16,
    ) -> Result<Option<(u16, Option<u16>, FreeBlock)>> {
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

    pub(super) fn read_free_block(&self, offset: u16) -> Result<FreeBlock> {
        let offset = self.free_block_offset(offset)?;
        let mut read_bytes = [0u8; FREE_BLOCK_SIZE];
        read_bytes.copy_from_slice(&self.data[offset..offset + FREE_BLOCK_SIZE]);
        Ok(FreeBlock::from_bytes(read_bytes))
    }

    pub(super) fn write_free_block(&mut self, offset: u16, block: &FreeBlock) -> Result<()> {
        let offset = self.free_block_offset(offset)?;
        self.data[offset..offset + FREE_BLOCK_SIZE].copy_from_slice(&block.to_bytes());
        Ok(())
    }

    pub(super) fn free_block_offset(&self, offset: u16) -> Result<usize> {
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

    pub(super) fn try_allocate_from_free_end(
        &mut self,
        allocate_len: usize,
    ) -> Result<Option<u16>> {
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
    ) -> Result<Option<u16>> {
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
