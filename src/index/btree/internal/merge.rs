use crate::{
    index::btree::{
        BTreeKeyType,
        header::BTreePageHeader,
        internal::{
            InternalPageError, append_internal_entry, initialize_internal_page,
            read_internal_entries,
        },
    },
    storage::page::{Page, PageId},
};

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
