use std::cmp::Ordering;

use thiserror::Error;

use crate::{
    index::{
        header::{BTreePageHeader, INTERNAL_HEADER_SIZE},
        key::{BTreeKey, BTreeKeyType},
    },
    page::{Page, PageId},
};

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

pub fn find_internal_child(
    page: &Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
) -> Result<PageId, InternalPageError> {
    let header =
        BTreePageHeader::from_bytes(page.as_bytes()).ok_or(InternalPageError::InvalidPage)?;
    if header.is_leaf() {
        return Err(InternalPageError::InvalidPage);
    }

    let entries = read_internal_entries(page, key_type)?;
    for entry in &entries {
        match entry.separator_key.compare(target) {
            Some(Ordering::Equal) | Some(Ordering::Greater) => return Ok(entry.left_child),
            Some(Ordering::Less) => continue,
            None => return Err(InternalPageError::InvalidPage),
        }
    }

    header
        .rightmost_child()
        .ok_or(InternalPageError::InvalidPage)
}

pub fn internal_split(
    left_page: &mut Page,
    right_page: &mut Page,
    key_type: BTreeKeyType,
) -> Result<BTreeKey, InternalPageError> {
    let mut entries = read_internal_entries(left_page, key_type)?;
    let header =
        BTreePageHeader::read_from_page(left_page).ok_or(InternalPageError::InvalidPage)?;

    let mut split_entries = entries.split_off(entries.len() / 2);

    if split_entries.is_empty() {
        return Err(InternalPageError::InvalidPage);
    }
    let promoted_entry = split_entries.remove(0);

    let old_rightmost = header
        .rightmost_child()
        .ok_or(InternalPageError::InvalidPage)?;
    let left_rightmost = promoted_entry.left_child;
    let separator_key = promoted_entry.separator_key;

    initialize_internal_page(left_page, left_rightmost);
    for entry in &entries {
        append_internal_entry(left_page, entry)?;
    }

    initialize_internal_page(right_page, old_rightmost);
    for entry in &split_entries {
        append_internal_entry(right_page, entry)?;
    }

    Ok(separator_key)
}

pub fn replace_child_after_leaf_split(
    parent: &mut Page,
    key_type: BTreeKeyType,
    old_child: PageId,
    left_child: PageId,
    right_child: PageId,
    separator: BTreeKey,
) -> Result<(), InternalPageError> {
    let mut entries = read_internal_entries(parent, key_type)?;
    let rightmost = BTreePageHeader::read_from_page(parent)
        .and_then(|header| header.rightmost_child())
        .ok_or(InternalPageError::InvalidPage)?;

    let new_rightmost = match entries
        .iter()
        .position(|entry| entry.left_child == old_child)
    {
        Some(index) => {
            let old_separator = entries[index].separator_key.clone();
            entries[index] = InternalEntry::new(left_child, separator);
            entries.insert(index + 1, InternalEntry::new(right_child, old_separator));
            rightmost
        }
        None if rightmost == old_child => {
            entries.push(InternalEntry::new(left_child, separator));
            right_child
        }
        None => return Err(InternalPageError::InvalidPage),
    };

    let mut rewritten = Page::new_raw();
    initialize_internal_page(&mut rewritten, new_rightmost);
    for entry in &entries {
        append_internal_entry(&mut rewritten, entry)?;
    }

    parent.as_bytes_mut().copy_from_slice(rewritten.as_bytes());
    Ok(())
}

pub fn remove_right_child_after_leaf_merge(
    parent: &mut Page,
    left_child: PageId,
    right_child: PageId,
    key_type: BTreeKeyType,
) -> Result<(), InternalPageError> {
    let mut entries = read_internal_entries(parent, key_type)?;
    let rightmost = BTreePageHeader::read_from_page(parent)
        .and_then(|header| header.rightmost_child())
        .ok_or(InternalPageError::InvalidPage)?;

    let left_index = entries
        .iter()
        .position(|entry| entry.left_child == left_child)
        .ok_or(InternalPageError::InvalidPage)?;

    let new_rightmost = if rightmost == right_child && left_index + 1 == entries.len() {
        entries.remove(left_index);
        left_child
    } else if let Some(entry) = entries.get(left_index + 1)
        && entry.left_child == right_child
    {
        let next = entries.remove(left_index + 1);
        entries[left_index].separator_key = next.separator_key;
        rightmost
    } else {
        return Err(InternalPageError::InvalidPage);
    };

    let mut temp_page = Page::new();
    initialize_internal_page(&mut temp_page, new_rightmost);
    for entry in &entries {
        append_internal_entry(&mut temp_page, entry)?;
    }
    *parent = temp_page;

    Ok(())
}

pub fn find_leaf_merge_pair(
    parent: &Page,
    key_type: BTreeKeyType,
    target: PageId,
) -> Result<Option<(PageId, PageId)>, InternalPageError> {
    let entries = read_internal_entries(parent, key_type)?;
    let rightmost = BTreePageHeader::read_from_page(parent)
        .and_then(|header| header.rightmost_child())
        .ok_or(InternalPageError::InvalidPage)?;

    let target_index = entries.iter().position(|entry| entry.left_child == target);

    if let Some(index) = target_index {
        if entries.len() > index + 1 {
            Ok(Some((target, entries[index + 1].left_child)))
        } else {
            Ok(Some((target, rightmost)))
        }
    } else if target == rightmost {
        if let Some(last) = entries.last() {
            Ok(Some((last.left_child, target)))
        } else {
            Ok(None)
        }
    } else {
        Err(InternalPageError::InvalidPage)
    }
}

#[derive(Debug, Error)]
pub enum InternalPageError {
    #[error("internal page가 손상되었습니다")]
    InvalidPage,
    #[error("internal page에 공간이 없습니다")]
    PageFull,
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
    fn internal_page에_entry를_추가하고_읽는다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(42));
        let entry = InternalEntry::new(PageId::new(3), BTreeKey::Int(42));
        let entry_bytes = vec![0, 0, 0, 0, 0, 0, 0, 3, 0, 4, 0, 0, 0, 42];

        // When
        append_internal_entry(&mut page, &entry)?;
        let entries = read_internal_entries(&page, BTreeKeyType::Int)?;

        // Then
        assert_eq!(
            &page.as_bytes()[0..13],
            &[1, 0, 1, 0, 27, 0, 0, 0, 0, 0, 0, 0, 42]
        );
        assert_eq!(&page.as_bytes()[13..27], entry_bytes);
        assert_eq!(
            entries
                .into_iter()
                .next()
                .and_then(|entry| entry.to_bytes()),
            Some(entry_bytes)
        );
        Ok(())
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
        assert!(matches!(entries, Err(InternalPageError::InvalidPage)));
    }

    #[test]
    fn internal_page가_가득차면_entry를_추가하지_않는다() {
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(42));
        let entry = InternalEntry::new(PageId::new(3), BTreeKey::Varchar("x".repeat(8170)));
        let page_before_append = page.as_bytes().to_vec();

        let result = append_internal_entry(&mut page, &entry);

        assert!(matches!(result, Err(InternalPageError::PageFull)));
        assert_eq!(page.as_bytes(), page_before_append);
    }

    #[test]
    fn internal_page를_split하고_중간_separator를반환한다() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut left_page = Page::new_raw();
        initialize_internal_page(&mut left_page, PageId::new(70));
        for (child_page_id, separator_key) in [
            (PageId::new(10), BTreeKey::Int(20)),
            (PageId::new(30), BTreeKey::Int(40)),
            (PageId::new(50), BTreeKey::Int(60)),
        ] {
            append_internal_entry(
                &mut left_page,
                &InternalEntry::new(child_page_id, separator_key),
            )?;
        }
        let mut right_page = Page::new_raw();

        let separator = internal_split(&mut left_page, &mut right_page, BTreeKeyType::Int)?;

        let left_header =
            BTreePageHeader::read_from_page(&left_page).ok_or(InternalPageError::InvalidPage)?;
        let right_header =
            BTreePageHeader::read_from_page(&right_page).ok_or(InternalPageError::InvalidPage)?;
        assert_eq!(separator, BTreeKey::Int(40));
        assert_eq!(left_header.rightmost_child(), Some(PageId::new(30)));
        assert_eq!(right_header.rightmost_child(), Some(PageId::new(70)));
        assert_eq!(
            read_internal_entries(&left_page, BTreeKeyType::Int)?
                .into_iter()
                .map(|entry| (entry.left_child, entry.separator_key))
                .collect::<Vec<_>>(),
            vec![(PageId::new(10), BTreeKey::Int(20))]
        );
        assert_eq!(
            read_internal_entries(&right_page, BTreeKeyType::Int)?
                .into_iter()
                .map(|entry| (entry.left_child, entry.separator_key))
                .collect::<Vec<_>>(),
            vec![(PageId::new(50), BTreeKey::Int(60))]
        );
        Ok(())
    }

    #[test]
    fn 일반_child_leaf_split_결과로_internal_page를_재구성한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(20)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(40)),
        )?;

        replace_child_after_leaf_split(
            &mut parent,
            BTreeKeyType::Int,
            PageId::new(20),
            PageId::new(21),
            PageId::new(22),
            BTreeKey::Int(30),
        )?;

        let header =
            BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
        assert_eq!(header.rightmost_child(), Some(PageId::new(30)));
        assert_eq!(
            read_internal_entries(&parent, BTreeKeyType::Int)?
                .into_iter()
                .map(|entry| (entry.left_child, entry.separator_key))
                .collect::<Vec<_>>(),
            vec![
                (PageId::new(10), BTreeKey::Int(20)),
                (PageId::new(21), BTreeKey::Int(30)),
                (PageId::new(22), BTreeKey::Int(40)),
            ]
        );
        Ok(())
    }

    #[test]
    fn rightmost_child_leaf_split_결과로_internal_page를_재구성한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(20)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(40)),
        )?;

        replace_child_after_leaf_split(
            &mut parent,
            BTreeKeyType::Int,
            PageId::new(30),
            PageId::new(31),
            PageId::new(32),
            BTreeKey::Int(60),
        )?;

        let header =
            BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
        assert_eq!(header.rightmost_child(), Some(PageId::new(32)));
        assert_eq!(
            read_internal_entries(&parent, BTreeKeyType::Int)?
                .into_iter()
                .map(|entry| (entry.left_child, entry.separator_key))
                .collect::<Vec<_>>(),
            vec![
                (PageId::new(10), BTreeKey::Int(20)),
                (PageId::new(20), BTreeKey::Int(40)),
                (PageId::new(31), BTreeKey::Int(60)),
            ]
        );
        Ok(())
    }

    #[test]
    fn 중간_right_child를_제거하면_left_separator를_갱신한다()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(40));
        for (child, separator) in [
            (PageId::new(10), BTreeKey::Int(10)),
            (PageId::new(20), BTreeKey::Int(20)),
            (PageId::new(30), BTreeKey::Int(30)),
        ] {
            append_internal_entry(&mut parent, &InternalEntry::new(child, separator))?;
        }

        // When
        remove_right_child_after_leaf_merge(
            &mut parent,
            PageId::new(10),
            PageId::new(20),
            BTreeKeyType::Int,
        )?;

        // Then
        assert_eq!(
            read_internal_entries(&parent, BTreeKeyType::Int)?
                .into_iter()
                .map(|entry| (entry.left_child, entry.separator_key))
                .collect::<Vec<_>>(),
            vec![
                (PageId::new(10), BTreeKey::Int(20)),
                (PageId::new(30), BTreeKey::Int(30)),
            ]
        );
        assert_eq!(
            find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(20))?,
            PageId::new(10)
        );
        assert_eq!(
            find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(21))?,
            PageId::new(30)
        );
        assert_eq!(
            find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(31))?,
            PageId::new(40)
        );
        Ok(())
    }

    #[test]
    fn rightmost_child를_제거하면_left_child가_새_rightmost가_된다()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
        )?;

        // When
        remove_right_child_after_leaf_merge(
            &mut parent,
            PageId::new(20),
            PageId::new(30),
            BTreeKeyType::Int,
        )?;

        // Then
        let header =
            BTreePageHeader::read_from_page(&parent).ok_or(InternalPageError::InvalidPage)?;
        assert_eq!(header.rightmost_child(), Some(PageId::new(20)));
        assert_eq!(
            find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(10))?,
            PageId::new(10)
        );
        assert_eq!(
            find_internal_child(&parent, BTreeKeyType::Int, &BTreeKey::Int(11))?,
            PageId::new(20)
        );
        Ok(())
    }

    #[test]
    fn 인접하지_않은_child는_병합하지_않고_parent를_유지한다()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
        )?;
        let parent_before_merge = parent.as_bytes().to_vec();

        // When
        let result = remove_right_child_after_leaf_merge(
            &mut parent,
            PageId::new(10),
            PageId::new(30),
            BTreeKeyType::Int,
        );

        // Then
        assert!(matches!(result, Err(InternalPageError::InvalidPage)));
        assert_eq!(parent.as_bytes(), parent_before_merge);
        Ok(())
    }

    #[test]
    fn 첫_child의_병합_pair는_오른쪽_sibling을_포함한다() -> Result<(), Box<dyn std::error::Error>>
    {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
        )?;

        // When
        let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(10))?;

        // Then
        assert_eq!(pair, Some((PageId::new(10), PageId::new(20))));
        Ok(())
    }

    #[test]
    fn rightmost_child의_병합_pair는_왼쪽_sibling을_포함한다()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
        )?;
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(20)),
        )?;

        // When
        let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(30))?;

        // Then
        assert_eq!(pair, Some((PageId::new(20), PageId::new(30))));
        Ok(())
    }

    #[test]
    fn child가_하나인_parent는_병합_pair가_없다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(10));

        // When
        let pair = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(10))?;

        // Then
        assert_eq!(pair, None);
        Ok(())
    }

    #[test]
    fn parent에_없는_child는_병합_pair를_찾을수_없다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(20));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(10)),
        )?;

        // When
        let result = find_leaf_merge_pair(&mut parent, BTreeKeyType::Int, PageId::new(30));

        // Then
        assert!(matches!(result, Err(InternalPageError::InvalidPage)));
        Ok(())
    }

    #[test]
    fn internal_page에_공간이_없으면_child_교체를_적용하지_않는다()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut parent = Page::new_raw();
        initialize_internal_page(&mut parent, PageId::new(30));
        append_internal_entry(
            &mut parent,
            &InternalEntry::new(PageId::new(10), BTreeKey::Varchar("a".repeat(8160))),
        )?;
        let parent_before_replace = parent.as_bytes().to_vec();

        let result = replace_child_after_leaf_split(
            &mut parent,
            BTreeKeyType::Varchar,
            PageId::new(10),
            PageId::new(11),
            PageId::new(12),
            BTreeKey::Varchar("b".to_owned()),
        );

        assert!(matches!(result, Err(InternalPageError::PageFull)));
        assert_eq!(parent.as_bytes(), parent_before_replace);
        Ok(())
    }

    #[test]
    fn key에_맞는_internal_child를_선택한다() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut page = Page::new_raw();
        initialize_internal_page(&mut page, PageId::new(30));
        append_internal_entry(
            &mut page,
            &InternalEntry::new(PageId::new(10), BTreeKey::Int(42)),
        )?;
        append_internal_entry(
            &mut page,
            &InternalEntry::new(PageId::new(20), BTreeKey::Int(99)),
        )?;

        // When / Then
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(42))?,
            PageId::new(10)
        );
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(50))?,
            PageId::new(20)
        );
        assert_eq!(
            find_internal_child(&page, BTreeKeyType::Int, &BTreeKey::Int(100))?,
            PageId::new(30)
        );
        Ok(())
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
