use thiserror::Error;

use crate::{
    index::btree::{
        BTreeKeyType,
        header::{BTreePageHeader, INTERNAL_HEADER_SIZE},
        internal::InternalEntry,
    },
    storage::page::{Page, PageId},
};

#[derive(Debug, Error)]
pub enum InternalPageError {
    #[error("internal page가 손상되었습니다")]
    InvalidPage,
    #[error("internal page에 공간이 없습니다")]
    PageFull,
}

pub fn initialize_internal_page(page: &mut Page, rightmost_child: PageId) {
    page.as_bytes_mut().fill(0);
    BTreePageHeader::new_internal(rightmost_child).write_to_page(page);
}

pub fn append_internal_entry(
    page: &mut Page,
    entry: &InternalEntry,
) -> Result<(), InternalPageError> {
    let mut header = BTreePageHeader::read_from_page(page).ok_or(InternalPageError::InvalidPage)?;
    if header.is_leaf() {
        return Err(InternalPageError::InvalidPage);
    }

    let entry_bytes = entry.to_bytes().ok_or(InternalPageError::InvalidPage)?;
    let start = header.entry_end() as usize;
    let end = start
        .checked_add(entry_bytes.len())
        .ok_or(InternalPageError::InvalidPage)?;
    if end > page.as_bytes().len() {
        return Err(InternalPageError::PageFull);
    }

    header
        .append_entry(u16::try_from(entry_bytes.len()).map_err(|_| InternalPageError::InvalidPage)?)
        .ok_or(InternalPageError::PageFull)?;
    page.as_bytes_mut()[start..end].copy_from_slice(&entry_bytes);
    header.write_to_page(page);
    Ok(())
}

pub fn read_internal_entries(
    page: &Page,
    key_type: BTreeKeyType,
) -> Result<Vec<InternalEntry>, InternalPageError> {
    let header = BTreePageHeader::read_from_page(page).ok_or(InternalPageError::InvalidPage)?;
    if header.is_leaf() {
        return Err(InternalPageError::InvalidPage);
    }

    let page_bytes = page.as_bytes();
    if header.entry_end() as usize > page_bytes.len() {
        return Err(InternalPageError::InvalidPage);
    }
    let mut entries = Vec::new();
    let mut offset = INTERNAL_HEADER_SIZE as usize;
    for _ in 0..header.entry_count() {
        if offset + 10 > header.entry_end() as usize {
            return Err(InternalPageError::InvalidPage);
        }

        let key_len = u16::from_be_bytes([page_bytes[offset + 8], page_bytes[offset + 9]]);
        let entry_end = offset
            .checked_add(key_len as usize)
            .ok_or(InternalPageError::InvalidPage)?
            .checked_add(10)
            .ok_or(InternalPageError::InvalidPage)?;
        if entry_end > header.entry_end() as usize {
            return Err(InternalPageError::InvalidPage);
        }

        entries.push(
            InternalEntry::from_bytes(key_type, &page_bytes[offset..entry_end])
                .ok_or(InternalPageError::InvalidPage)?,
        );
        offset = entry_end;
    }

    if offset != header.entry_end() as usize {
        return Err(InternalPageError::InvalidPage);
    }

    Ok(entries)
}
