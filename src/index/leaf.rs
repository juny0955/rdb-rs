use std::cmp::Ordering;

use thiserror::Error;

use crate::{
    index::{
        header::BTreePageHeader,
        key::{BTreeKey, BTreeKeyType},
    },
    page::{Page, RowId},
};

pub fn initialize_leaf_page(page: &mut Page) {
    page.as_bytes_mut().fill(0);
    BTreePageHeader::new_leaf().write_to_page(page);
}

pub fn append_leaf_entry(page: &mut Page, entry: &LeafEntry) -> Result<(), LeafPageError> {
    let mut header = BTreePageHeader::read_from_page(page).ok_or(LeafPageError::InvalidPage)?;
    if !header.is_leaf() {
        return Err(LeafPageError::InvalidPage);
    }

    let entry_bytes = entry.to_bytes().ok_or(LeafPageError::InvalidPage)?;
    let start = header.entry_end() as usize;
    let end = start
        .checked_add(entry_bytes.len())
        .ok_or(LeafPageError::InvalidPage)?;
    if end > page.as_bytes().len() {
        return Err(LeafPageError::PageFull);
    }

    header
        .append_entry(u16::try_from(entry_bytes.len()).map_err(|_| LeafPageError::InvalidPage)?)
        .ok_or(LeafPageError::PageFull)?;
    page.as_bytes_mut()[start..end].copy_from_slice(&entry_bytes);
    header.write_to_page(page);
    Ok(())
}

pub fn read_leaf_entries(
    page: &Page,
    key_type: BTreeKeyType,
) -> Result<Vec<LeafEntry>, LeafPageError> {
    let header = BTreePageHeader::read_from_page(page).ok_or(LeafPageError::InvalidPage)?;
    if !header.is_leaf() {
        return Err(LeafPageError::InvalidPage);
    }

    let page_bytes = page.as_bytes();
    if header.entry_end() as usize > page_bytes.len() {
        return Err(LeafPageError::InvalidPage);
    }
    let mut entries = Vec::new();
    let mut offset = 5;
    for _ in 0..header.entry_count() {
        if offset + 2 > header.entry_end() as usize {
            return Err(LeafPageError::InvalidPage);
        }

        let key_len = u16::from_be_bytes([page_bytes[offset], page_bytes[offset + 1]]);
        let entry_end = offset
            .checked_add(2)
            .ok_or(LeafPageError::InvalidPage)?
            .checked_add(key_len as usize)
            .ok_or(LeafPageError::InvalidPage)?
            .checked_add(10)
            .ok_or(LeafPageError::InvalidPage)?;
        if entry_end > header.entry_end() as usize {
            return Err(LeafPageError::InvalidPage);
        }

        entries.push(
            LeafEntry::from_bytes(key_type, &page_bytes[offset..entry_end])
                .ok_or(LeafPageError::InvalidPage)?,
        );
        offset = entry_end;
    }

    if offset != header.entry_end() as usize {
        return Err(LeafPageError::InvalidPage);
    }

    Ok(entries)
}

pub fn find_leaf_entry(
    page: &Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
) -> Result<Option<RowId>, LeafPageError> {
    let entries = read_leaf_entries(page, key_type)?;
    for entry in &entries {
        if let Some(Ordering::Equal) = entry.key.compare(target) {
            return Ok(Some(entry.row_id));
        }
    }

    Ok(None)
}

pub fn leaf_split(
    left_page: &mut Page,
    right_page: &mut Page,
    key_type: BTreeKeyType,
    new_entry: LeafEntry,
) -> Result<BTreeKey, LeafPageError> {
    let mut entries = read_leaf_entries(left_page, key_type)?;
    entries.push(new_entry);

    if entries.iter().any(|entry| entry.key.key_type() != key_type) {
        return Err(LeafPageError::InvalidPage);
    }

    entries.sort_unstable_by(|left, right| {
        left.key
            .compare(&right.key)
            .expect("검증된 entry는 같은 key type이다")
    });

    let split_entries = entries.split_off(entries.len() / 2);
    let separator_key = entries
        .last()
        .ok_or(LeafPageError::InvalidPage)?
        .key
        .clone();

    initialize_leaf_page(left_page);
    for entry in &entries {
        append_leaf_entry(left_page, entry)?;
    }

    initialize_leaf_page(right_page);
    for entry in &split_entries {
        append_leaf_entry(right_page, entry)?;
    }

    Ok(separator_key)
}

#[derive(Debug, Error)]
pub enum LeafPageError {
    #[error("leaf page가 손상되었습니다")]
    InvalidPage,
    #[error("leaf page에 공간이 없습니다")]
    PageFull,
}

#[derive(Debug)]
pub struct LeafEntry {
    key: BTreeKey,
    row_id: RowId,
}

impl LeafEntry {
    pub fn new(key: BTreeKey, row_id: RowId) -> Self {
        Self { key, row_id }
    }

    pub fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }
        let key_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;

        if bytes.len() != 2 + key_len + 10 {
            return None;
        }

        let key_end = 2 + key_len;
        let key_bytes = &bytes[2..key_end];
        let key = BTreeKey::from_bytes(key_type, key_bytes)?;

        let row_id_start = 2 + key_len;
        let row_id_bytes: [u8; 10] = bytes[row_id_start..].try_into().ok()?;
        let row_id = RowId::from_bytes(row_id_bytes);

        Some(Self { key, row_id })
    }

    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let key_bytes = self.key.to_bytes();
        let key_len = u16::try_from(key_bytes.len()).ok()?;

        bytes.extend_from_slice(key_len.to_be_bytes().as_ref());
        bytes.extend_from_slice(&key_bytes);
        bytes.extend_from_slice(self.row_id.to_bytes().as_ref());
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Page, PageId, SlotId};

    #[test]
    fn int_leaf_entry를_바이트로_직렬화한다() {
        // Given
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );

        // When
        let bytes = entry.to_bytes();

        // Then
        assert_eq!(
            bytes,
            Some(vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7,])
        );
    }

    #[test]
    fn varchar_leaf_entry를_바이트로_직렬화한다() {
        // Given
        let entry = LeafEntry::new(
            BTreeKey::Varchar(String::from("hi")),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );

        // When
        let bytes = entry.to_bytes();

        // Then
        assert_eq!(
            bytes,
            Some(vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7])
        );
    }

    #[test]
    fn int_leaf_entry를_바이트에서_복원한다() {
        // Given
        let bytes = vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Int, &bytes);

        // Then
        assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
    }

    #[test]
    fn varchar_leaf_entry를_바이트에서_복원한다() {
        // Given
        let bytes = vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Varchar, &bytes);

        // Then
        assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
    }

    #[test]
    fn 잘못된_boolean_바이트를_거부한다() {
        // Given
        let bytes = [0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Boolean, &bytes);

        // Then
        assert!(entry.is_none());
    }

    #[test]
    fn leaf_page에_첫_entry를_추가한다() {
        // Given
        let mut page = Page::new_raw();
        initialize_leaf_page(&mut page);
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );

        // When
        let result = append_leaf_entry(&mut page, &entry);

        // Then
        result.expect("빈 leaf page에 entry를 추가할 수 있어야 한다");
        assert_eq!(&page.as_bytes()[0..5], &[0, 0, 1, 0, 21]);
        assert_eq!(
            &page.as_bytes()[5..21],
            &[0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7]
        );
    }

    #[test]
    fn leaf_page에서_entry를_읽는다() {
        // Given
        let mut page = Page::new_raw();
        initialize_leaf_page(&mut page);
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );
        let appended = append_leaf_entry(&mut page, &entry);

        // When
        let entries = read_leaf_entries(&page, BTreeKeyType::Int)
            .expect("정상 leaf page의 entry를 읽을 수 있어야 한다");

        // Then
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries
                .into_iter()
                .next()
                .and_then(|entry| entry.to_bytes()),
            Some(vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7])
        );
        appended.expect("정상 leaf page에 entry를 추가할 수 있어야 한다");
    }

    #[test]
    fn 존재하는_leaf_key를_찾는다() {
        // Given
        let mut page = Page::new_raw();
        initialize_leaf_page(&mut page);
        let row_id = RowId::new(PageId::new(3), SlotId::new(7));
        let entry = LeafEntry::new(BTreeKey::Int(42), row_id);
        append_leaf_entry(&mut page, &entry)
            .expect("정상 leaf page에 entry를 추가할 수 있어야 한다");

        // When
        let found = find_leaf_entry(&page, BTreeKeyType::Int, &BTreeKey::Int(42))
            .expect("정상 leaf page에서 key를 검색할 수 있어야 한다");

        // Then
        assert_eq!(found, Some(row_id));
    }

    #[test]
    fn 존재하지_않는_leaf_key는_찾지_못한다() {
        // Given
        let mut page = Page::new_raw();
        initialize_leaf_page(&mut page);
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );
        append_leaf_entry(&mut page, &entry)
            .expect("정상 leaf page에 entry를 추가할 수 있어야 한다");

        // When
        let found = find_leaf_entry(&page, BTreeKeyType::Int, &BTreeKey::Int(7))
            .expect("정상 leaf page에서 key를 검색할 수 있어야 한다");

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn leaf를_split하고_separator를반환한다() -> Result<(), Box<dyn std::error::Error>> {
        let mut left_page = Page::new_raw();
        initialize_leaf_page(&mut left_page);
        for (key, row_id) in [
            (
                BTreeKey::Int(10),
                RowId::new(PageId::new(1), SlotId::new(1)),
            ),
            (
                BTreeKey::Int(20),
                RowId::new(PageId::new(1), SlotId::new(2)),
            ),
            (
                BTreeKey::Int(30),
                RowId::new(PageId::new(1), SlotId::new(3)),
            ),
        ] {
            append_leaf_entry(&mut left_page, &LeafEntry::new(key, row_id))?;
        }
        let mut right_page = Page::new_raw();

        let separator = leaf_split(
            &mut left_page,
            &mut right_page,
            BTreeKeyType::Int,
            LeafEntry::new(
                BTreeKey::Int(25),
                RowId::new(PageId::new(1), SlotId::new(4)),
            ),
        )?;

        let left_entries = read_leaf_entries(&left_page, BTreeKeyType::Int)?;
        let right_entries = read_leaf_entries(&right_page, BTreeKeyType::Int)?;
        assert_eq!(separator, BTreeKey::Int(20));
        assert_eq!(
            left_entries
                .into_iter()
                .map(|entry| (entry.key, entry.row_id))
                .collect::<Vec<_>>(),
            vec![
                (
                    BTreeKey::Int(10),
                    RowId::new(PageId::new(1), SlotId::new(1)),
                ),
                (
                    BTreeKey::Int(20),
                    RowId::new(PageId::new(1), SlotId::new(2)),
                ),
            ]
        );
        assert_eq!(
            right_entries
                .into_iter()
                .map(|entry| (entry.key, entry.row_id))
                .collect::<Vec<_>>(),
            vec![
                (
                    BTreeKey::Int(25),
                    RowId::new(PageId::new(1), SlotId::new(4)),
                ),
                (
                    BTreeKey::Int(30),
                    RowId::new(PageId::new(1), SlotId::new(3)),
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn 공간이_부족하면_leaf_entry를_추가하지_않는다() {
        // Given
        let mut page = Page::new_raw();
        initialize_leaf_page(&mut page);
        let full_entry = LeafEntry::new(
            BTreeKey::Varchar("x".repeat(8175)),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );
        append_leaf_entry(&mut page, &full_entry)
            .expect("leaf page에 최대 entry를 추가할 수 있어야 한다");
        let page_before_append = page.as_bytes().to_vec();
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(4), SlotId::new(8)),
        );

        // When
        let result = append_leaf_entry(&mut page, &entry);

        // Then
        assert!(matches!(result, Err(LeafPageError::PageFull)));
        assert_eq!(page.as_bytes(), page_before_append);
        let entries = read_leaf_entries(&page, BTreeKeyType::Varchar)
            .expect("정상 leaf page의 entry를 읽을 수 있어야 한다");
        assert_eq!(
            entries
                .into_iter()
                .next()
                .and_then(|entry| entry.to_bytes()),
            full_entry.to_bytes()
        );
    }
}
