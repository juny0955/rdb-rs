use std::collections::HashMap;

use crate::{buffer::frame::FrameId, page::PageId};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PageTableError {
    AlreadyExistsPageId,
}

pub(super) struct PageTable {
    page_to_frame: HashMap<PageId, FrameId>,
}

impl PageTable {
    pub(super) fn new() -> Self {
        Self {
            page_to_frame: HashMap::new(),
        }
    }

    pub(super) fn insert(
        &mut self,
        page_id: PageId,
        frame_id: FrameId,
    ) -> Result<(), PageTableError> {
        if self.page_to_frame.contains_key(&page_id) {
            return Err(PageTableError::AlreadyExistsPageId);
        }
        self.page_to_frame.insert(page_id, frame_id);

        Ok(())
    }

    pub(super) fn get(&self, page_id: &PageId) -> Option<FrameId> {
        self.page_to_frame.get(page_id).copied()
    }

    pub(super) fn remove(&mut self, page_id: &PageId) -> Option<FrameId> {
        self.page_to_frame.remove(page_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert한_page_id를_조회한다() {
        let page_id = PageId::new(10);
        let frame_id = FrameId::new(0);
        let mut page_table = PageTable::new();

        assert!(page_table.insert(page_id, frame_id).is_ok());

        assert_eq!(page_table.get(&page_id), Some(frame_id));
    }

    #[test]
    fn 없는_page_id를_조회하면_none을_반환한다() {
        let page_table = PageTable::new();

        assert_eq!(page_table.get(&PageId::new(10)), None);
    }

    #[test]
    fn 제거한_page_id를_조회하면_none을_반환한다() {
        let page_id = PageId::new(10);
        let frame_id = FrameId::new(0);
        let mut page_table = PageTable::new();
        assert!(page_table.insert(page_id, frame_id).is_ok());

        let removed = page_table.remove(&page_id);

        assert_eq!(removed, Some(frame_id));
        assert_eq!(page_table.get(&page_id), None);
    }
}
