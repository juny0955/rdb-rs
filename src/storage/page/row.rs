use std::cmp::Ordering;

use crate::storage::page::{
    HEADER_SIZE, PAGE_SIZE, Page, PageError, Row, SLOT_SIZE, Slot, SlotId, row_allocation_size,
};

impl Page {
    pub fn insert_row(&mut self, row: &Row) -> Result<SlotId, PageError> {
        let row_bytes = row.to_bytes();
        let row_len = row_bytes.len();
        let allocate_len = row_allocation_size(row_len);

        if allocate_len > PAGE_SIZE - HEADER_SIZE - SLOT_SIZE {
            return Err(PageError::RowTooLarge);
        }

        self.try_insert_from_available_space(row_bytes, allocate_len)
    }

    pub fn read_row(&self, slot_id: SlotId) -> Result<Row, PageError> {
        let slot = self.read_slot(slot_id)?;
        let row_end = slot.offset as usize + slot.length as usize;
        if row_end > PAGE_SIZE || slot.offset < self.free_end() {
            return Err(PageError::InvalidRowBounds);
        }

        let row = Row::from_bytes(&self.data[slot.offset as usize..row_end]);
        Ok(row)
    }

    pub fn update_row(&mut self, slot_id: SlotId, row: &Row) -> Result<(), PageError> {
        let mut slot = self.read_slot(slot_id)?;
        let slot_length = slot.length as usize;
        let slot_offset = slot.offset as usize;
        let row_bytes = row.to_bytes();
        let old_allocate_len = row_allocation_size(slot_length);
        let new_allocate_len = row_allocation_size(row_bytes.len());
        let allocate_end = slot_offset + old_allocate_len;

        if allocate_end > PAGE_SIZE || slot_offset < self.free_end() as usize {
            return Err(PageError::InvalidRowBounds);
        }

        match new_allocate_len.cmp(&old_allocate_len) {
            Ordering::Equal => {
                self.update_allocate_len_equal(slot_offset, row_bytes, slot_id, &mut slot)
            }
            Ordering::Less => self.update_allocate_len_less(
                slot_offset,
                row_bytes,
                slot_id,
                &mut slot,
                old_allocate_len,
                new_allocate_len,
            ),
            Ordering::Greater => self.update_allocate_len_greater(
                row_bytes,
                slot_id,
                slot,
                old_allocate_len,
                new_allocate_len,
            ),
        }
    }

    pub fn delete_row(&mut self, slot_id: SlotId) -> Result<(), PageError> {
        let mut slot = self.read_slot(slot_id)?;
        let row_offset = slot.offset;
        let allocate_len = row_allocation_size(slot.length as usize);

        self.add_free_block(row_offset, allocate_len as u16)?;

        slot.tombstone();
        self.write_slot(slot_id, &slot)?;
        Ok(())
    }

    pub fn scan_rows(&self) -> Result<Vec<(SlotId, Row)>, PageError> {
        let mut scans = Vec::new();

        let slot_count = self.slot_count();
        for i in 0..slot_count {
            let slot_id = SlotId(i);
            match self.read_row(slot_id) {
                Ok(row) => scans.push((slot_id, row)),
                Err(PageError::SlotNotFound) => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(scans)
    }

    fn try_insert_from_available_space(
        &mut self,
        row_bytes: &[u8],
        allocate_len: usize,
    ) -> Result<SlotId, PageError> {
        if let Some(slot_id) = self.try_insert_from_free_block(row_bytes, allocate_len)? {
            return Ok(slot_id);
        }

        match self.insert_from_free_end(row_bytes, allocate_len) {
            Ok(slot_id) => Ok(slot_id),
            Err(PageError::StorageFull) => {
                self.compact()?;
                Ok(self.insert_from_free_end(row_bytes, allocate_len)?)
            }
            Err(e) => Err(e),
        }
    }

    fn update_allocate_len_equal(
        &mut self,
        slot_offset: usize,
        row_bytes: &[u8],
        slot_id: SlotId,
        slot: &mut Slot,
    ) -> Result<(), PageError> {
        self.update_slot_for_row(slot_id, slot, row_bytes.len())?;
        self.write_row_bytes_at(slot_offset as u16, row_bytes);
        Ok(())
    }

    fn update_allocate_len_less(
        &mut self,
        slot_offset: usize,
        row_bytes: &[u8],
        slot_id: SlotId,
        slot: &mut Slot,
        old_allocate_len: usize,
        new_allocate_len: usize,
    ) -> Result<(), PageError> {
        self.write_row_bytes_at(slot.offset, row_bytes);

        self.update_slot_for_row(slot_id, slot, row_bytes.len())?;

        let free_block_length = old_allocate_len - new_allocate_len;
        let free_block_offset = slot_offset + new_allocate_len;
        self.add_free_block(free_block_offset as u16, free_block_length as u16)?;
        Ok(())
    }

    fn update_allocate_len_greater(
        &mut self,
        row_bytes: &[u8],
        slot_id: SlotId,
        mut slot: Slot,
        mut old_allocate_len: usize,
        new_allocate_len: usize,
    ) -> Result<(), PageError> {
        let new_offset =
            if let Some(offset) = self.try_allocate_from_free_block(new_allocate_len)? {
                offset
            } else if let Some(offset) = self.try_allocate_from_free_end(new_allocate_len)? {
                offset
            } else {
                self.compact()?;
                slot = self.read_slot(slot_id)?;
                old_allocate_len = row_allocation_size(slot.length as usize);

                self.try_allocate_from_free_end(new_allocate_len)?
                    .ok_or(PageError::StorageFull)?
            };

        self.write_row_bytes_at(new_offset, row_bytes);

        let old_slot_offset = slot.offset;
        slot.offset = new_offset;
        self.update_slot_for_row(slot_id, &mut slot, row_bytes.len())?;

        self.add_free_block(old_slot_offset, old_allocate_len as u16)?;
        Ok(())
    }

    fn update_slot_for_row(
        &mut self,
        slot_id: SlotId,
        slot: &mut Slot,
        row_len: usize,
    ) -> Result<(), PageError> {
        slot.length = row_len as u16;
        self.write_slot(slot_id, slot)?;
        Ok(())
    }

    fn try_insert_from_free_block(
        &mut self,
        row_bytes: &[u8],
        allocate_len: usize,
    ) -> Result<Option<SlotId>, PageError> {
        if SLOT_SIZE > self.free_space()? {
            return Ok(None);
        }

        self.try_allocate_from_free_block(allocate_len)?
            .map(|offset| self.write_row_at(offset, row_bytes))
            .transpose()
    }

    fn insert_from_free_end(
        &mut self,
        row_bytes: &[u8],
        allocate_len: usize,
    ) -> Result<SlotId, PageError> {
        if SLOT_SIZE + allocate_len > self.free_space()? {
            return Err(PageError::StorageFull);
        }

        let row_end = self.free_end();
        let row_start = row_end - allocate_len as u16;

        let slot_id = self.write_row_at(row_start, row_bytes)?;
        self.set_free_end(row_start);

        Ok(slot_id)
    }

    fn write_row_at(&mut self, offset: u16, row_bytes: &[u8]) -> Result<SlotId, PageError> {
        let slot = Slot::new(offset, row_bytes.len() as u16);
        let slot_id = self.add_slot(&slot)?;

        self.write_row_bytes_at(offset, row_bytes);
        Ok(slot_id)
    }

    fn write_row_bytes_at(&mut self, offset: u16, row_bytes: &[u8]) {
        let offset = offset as usize;
        self.data[offset..offset + row_bytes.len()].copy_from_slice(row_bytes);
    }
}
