use std::cmp::Ordering;

use crate::{
    index::{
        header::BTreePageHeader,
        key::{BTreeKey, BTreeKeyType},
    },
    page::{Page, PageId},
};

pub fn initialize_internal_page(page: &mut Page, rightmost_child: PageId) {
    page.as_bytes_mut().fill(0);
    BTreePageHeader::new_internal(rightmost_child).write_to_page(page);
}

pub fn append_internal_entry(page: &mut Page, entry: &InternalEntry) -> Option<()> {
    let mut header = BTreePageHeader::read_from_page(page)?;
    if header.is_leaf() {
        return None;
    }

    let entry_bytes = entry.to_bytes()?;
    let start = header.entry_end() as usize;
    let end = start.checked_add(entry_bytes.len())?;
    if end > page.as_bytes().len() {
        return None;
    }

    header.append_entry(u16::try_from(entry_bytes.len()).ok()?)?;
    page.as_bytes_mut()[start..end].copy_from_slice(&entry_bytes);
    header.write_to_page(page);
    Some(())
}

pub fn read_internal_entries(page: &Page, key_type: BTreeKeyType) -> Option<Vec<InternalEntry>> {
    let header = BTreePageHeader::read_from_page(page)?;
    if header.is_leaf() {
        return None;
    }

    let page_bytes = page.as_bytes();
    if header.entry_end() as usize > page_bytes.len() {
        return None;
    }
    let mut entries = Vec::new();
    let mut offset = 13;
    for _ in 0..header.entry_count() {
        if offset + 10 > header.entry_end() as usize {
            return None;
        }

        let key_len = u16::from_be_bytes([page_bytes[offset + 8], page_bytes[offset + 9]]);
        let entry_end = offset.checked_add(key_len as usize)?.checked_add(10)?;
        if entry_end > header.entry_end() as usize {
            return None;
        }

        entries.push(InternalEntry::from_bytes(
            key_type,
            &page_bytes[offset..entry_end],
        )?);
        offset = entry_end;
    }

    if offset != header.entry_end() as usize {
        return None;
    }

    Some(entries)
}

pub fn find_internal_child(
    page: &Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
) -> Option<PageId> {
    let header = BTreePageHeader::from_bytes(page.as_bytes())?;
    if header.is_leaf() {
        return None;
    }

    let entries = read_internal_entries(page, key_type)?;
    for entry in &entries {
        match entry.separator_key.compare(target) {
            Some(Ordering::Equal) | Some(Ordering::Greater) => return Some(entry.left_child),
            Some(Ordering::Less) => continue,
            None => return None,
        }
    }

    header.rightmost_child()
}

pub struct InternalEntry {
    left_child: PageId,
    separator_key: BTreeKey,
}

impl InternalEntry {
    pub fn new(left_child: PageId, separator_key: BTreeKey) -> Self {
        Self {
            left_child,
            separator_key,
        }
    }

    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let left_bytes = self.left_child.to_bytes();
        let key_bytes = self.separator_key.to_bytes();
        let key_len = u16::try_from(key_bytes.len()).ok()?;

        bytes.extend_from_slice(left_bytes.as_ref());
        bytes.extend_from_slice(&key_len.to_be_bytes());
        bytes.extend_from_slice(&key_bytes);
        Some(bytes)
    }

    pub fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 {
            return None;
        }
        let left_child = PageId::from_bytes(bytes[0..8].try_into().ok()?);
        let key_len = u16::from_be_bytes(bytes[8..10].try_into().ok()?) as usize;

        if bytes.len() != key_len + 10 {
            return None;
        }

        let separator_key = BTreeKey::from_bytes(key_type, &bytes[10..])?;
        Some(Self {
            left_child,
            separator_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Page;

    #[test]
    fn internal_page를_초기화한다() {
        // Given
        let mut page = Page::new_raw();

        // When
        initialize_internal_page(&mut page, PageId::new(42));

        // Then
        assert_eq!(
            &page.as_bytes()[0..13],
            &[1, 0, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 42]
        );
    }

    #[test]
    fn internal_page에_entry를_추가하고_읽는다() {
        // Given
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(42));
        let entry = InternalEntry::new(PageId::new(3), BTreeKey::Int(42));
        let entry_bytes = vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42];

        // When
        let appended = append_internal_entry(&mut page, &entry);
        let entries = read_internal_entries(&page, BTreeKeyType::Int);

        // Then
        assert_eq!(appended, Some(()));
        assert_eq!(
            &page.as_bytes()[0..13],
            &[1, 0, 1, 0, 27, 0, 0, 0, 0, 0, 0, 0, 42]
        );
        assert_eq!(&page.as_bytes()[13..27], entry_bytes);
        assert_eq!(
            entries
                .and_then(|entries| entries.into_iter().next())
                .and_then(|entry| entry.to_bytes()),
            Some(entry_bytes)
        );
    }

    #[test]
    fn 고정_헤더보다_짧은_internal_entry는_읽기를_거부한다() {
        // Given
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(42));
        page.as_bytes_mut()[1..3].copy_from_slice(&1_u16.to_be_bytes());
        page.as_bytes_mut()[3..5].copy_from_slice(&21_u16.to_be_bytes());

        // When
        let entries = read_internal_entries(&page, BTreeKeyType::Int);

        // Then
        assert!(entries.is_none());
    }

    #[test]
    fn key에_맞는_internal_child를_선택한다() {
        // Given
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(30));
        assert_eq!(
            append_internal_entry(
                &mut page,
                &InternalEntry::new(PageId::new(10), BTreeKey::Int(42)),
            ),
            Some(())
        );
        assert_eq!(
            append_internal_entry(
                &mut page,
                &InternalEntry::new(PageId::new(20), BTreeKey::Int(99)),
            ),
            Some(())
        );

        // When / Then
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(42)),
            Some(PageId::new(10))
        );
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(50)),
            Some(PageId::new(20))
        );
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(100)),
            Some(PageId::new(30))
        );
    }

    #[test]
    fn int_internal_entry를_바이트로_직렬화한다() {
        // Given
        let entry = InternalEntry::new(PageId::new(3), BTreeKey::Int(42));

        // When
        let bytes = entry.to_bytes();

        // Then
        assert_eq!(bytes, Some(vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42]));
    }

    #[test]
    fn int_internal_entry를_바이트에서_복원한다() {
        // Given
        let bytes = vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42];

        // When
        let entry = InternalEntry::from_bytes(BTreeKeyType::Int, &bytes);

        // Then
        assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
    }
}
