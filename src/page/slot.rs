use std::io::{Error, ErrorKind, Result};

use crate::page::{HEADER_SIZE, PAGE_SIZE, Page, SLOT_SIZE};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SlotId(pub(super) u16);

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Slot {
    pub(super) offset: u16,
    pub(super) length: u16,
}

impl Slot {
    pub(super) fn new(offset: u16, length: u16) -> Self {
        Self { offset, length }
    }

    pub(super) fn from_bytes(bytes: [u8; SLOT_SIZE]) -> Self {
        let offset = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = u16::from_be_bytes([bytes[2], bytes[3]]);

        Self { offset, length }
    }

    pub(super) fn to_bytes(&self) -> [u8; SLOT_SIZE] {
        let offset = self.offset.to_be_bytes();
        let length = self.length.to_be_bytes();

        [offset[0], offset[1], length[0], length[1]]
    }

    pub(super) fn tombstone(&mut self) {
        self.offset = 0;
        self.length = 0;
    }

    pub(super) fn is_deleted(&self) -> bool {
        self.offset == 0 && self.length == 0
    }
}

impl Page {
    pub(super) fn add_slot(&mut self, slot: &Slot) -> Result<SlotId> {
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

    pub(super) fn read_slot(&self, slot_id: SlotId) -> Result<Slot> {
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

    pub(super) fn write_slot(&mut self, slot_id: SlotId, slot: &Slot) -> Result<()> {
        let offset = slot_offset(slot_id)?;
        let bytes = slot.to_bytes();

        self.data[offset..offset + SLOT_SIZE].copy_from_slice(&bytes);

        Ok(())
    }
}

pub(super) fn slot_offset(slot_id: SlotId) -> Result<usize> {
    let offset = HEADER_SIZE + (SLOT_SIZE * slot_id.0 as usize);
    if offset > PAGE_SIZE || offset + SLOT_SIZE > PAGE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "slot offset over page size",
        ));
    }

    Ok(offset)
}
