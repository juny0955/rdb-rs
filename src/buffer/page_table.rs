use std::collections::HashMap;

use crate::buffer::{frame::FrameId, page_key::PageKey};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PageTableError {
    AlreadyExistsPageId,
}

#[derive(Debug)]
pub(super) struct PageTable {
    table: HashMap<PageKey, FrameId>,
}

impl PageTable {
    pub(super) fn new() -> Self {
        Self {
            table: HashMap::new(),
        }
    }

    pub(super) fn insert(
        &mut self,
        page_key: PageKey,
        frame_id: FrameId,
    ) -> Result<(), PageTableError> {
        if self.table.contains_key(&page_key) {
            return Err(PageTableError::AlreadyExistsPageId);
        }
        self.table.insert(page_key, frame_id);

        Ok(())
    }

    pub(super) fn get(&self, page_key: &PageKey) -> Option<FrameId> {
        self.table.get(page_key).copied()
    }

    pub(super) fn remove(&mut self, page_key: &PageKey) -> Option<FrameId> {
        self.table.remove(page_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buffer::page_key::PageKey,
        page::PageId,
        schema::{RelationId, TableId},
    };

    fn page_key(table_id: u32, page_id: u64) -> PageKey {
        PageKey::new(
            RelationId::Heap(TableId::new(table_id)),
            PageId::new(page_id),
        )
    }

    #[test]
    fn insert한_page_key를_조회한다() {
        let page_key = page_key(1, 10);
        let frame_id = FrameId::new(0);
        let mut page_table = PageTable::new();

        assert!(page_table.insert(page_key, frame_id).is_ok());

        assert_eq!(page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn 없는_page_key를_조회하면_none을_반환한다() {
        let page_table = PageTable::new();

        assert_eq!(page_table.get(&page_key(1, 10)), None);
    }

    #[test]
    fn 제거한_page_key를_조회하면_none을_반환한다() {
        let page_key = page_key(1, 10);
        let frame_id = FrameId::new(0);
        let mut page_table = PageTable::new();
        assert!(page_table.insert(page_key, frame_id).is_ok());

        let removed = page_table.remove(&page_key);

        assert_eq!(removed, Some(frame_id));
        assert_eq!(page_table.get(&page_key), None);
    }

    #[test]
    fn table_id가_다르면_같은_page_id도_별도로_관리한다() {
        let first = page_key(1, 10);
        let second = page_key(2, 10);
        let mut page_table = PageTable::new();

        page_table
            .insert(first, FrameId::new(0))
            .expect("첫 PageKey는 삽입되어야 한다");
        page_table
            .insert(second, FrameId::new(1))
            .expect("다른 table의 같은 PageId는 삽입되어야 한다");

        assert_eq!(page_table.get(&first), Some(FrameId::new(0)));
        assert_eq!(page_table.get(&second), Some(FrameId::new(1)));
    }
}
