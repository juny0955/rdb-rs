use crate::{storage::buffer::page_key::PageKey, storage::page::Page};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) struct FrameId(usize);

impl FrameId {
    pub(super) fn new(id: usize) -> Self {
        Self(id)
    }

    pub(super) fn index(&self) -> usize {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum BufferFrameError {
    AlreadyUnpinned,
}

#[derive(Debug)]
pub(super) struct BufferFrame {
    page_key: PageKey,
    page: Page,
    pin_count: usize,
    is_dirty: bool,
    referenced: bool,
}

impl BufferFrame {
    pub(super) fn new(page_key: PageKey, page: Page) -> Self {
        Self {
            page_key,
            page,
            pin_count: 1,
            is_dirty: false,
            referenced: true,
        }
    }

    pub(super) fn pin(&mut self) {
        self.pin_count += 1;
        self.referenced = true;
    }

    pub(super) fn unpin(&mut self) -> Result<(), BufferFrameError> {
        if self.pin_count == 0 {
            return Err(BufferFrameError::AlreadyUnpinned);
        }

        self.pin_count -= 1;
        Ok(())
    }

    pub(super) fn mark_clean(&mut self) {
        self.is_dirty = false;
    }

    pub(super) fn unreferenced(&mut self) {
        self.referenced = false;
    }

    pub(super) fn pin_count(&self) -> usize {
        self.pin_count
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub(super) fn is_referenced(&self) -> bool {
        self.referenced
    }

    pub(super) fn page_key(&self) -> PageKey {
        self.page_key
    }

    pub fn page(&self) -> &Page {
        &self.page
    }

    pub fn page_mut(&mut self) -> &mut Page {
        self.is_dirty = true;
        &mut self.page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        schema::{RelationId, TableId},
        storage::buffer::page_key::PageKey,
        storage::page::PageId,
    };

    fn page_key(page_id: u64) -> PageKey {
        PageKey::new(RelationId::Heap(TableId::new(1)), PageId::new(page_id))
    }

    #[test]
    fn pin_unpin_테스트() {
        let mut frame = BufferFrame::new(page_key(0), Page::new());

        frame.pin();
        assert_eq!(frame.pin_count, 2);

        assert!(frame.unpin().is_ok());
        assert_eq!(frame.pin_count, 1);
        assert!(frame.unpin().is_ok());
        assert_eq!(frame.pin_count, 0);
        assert!(matches!(
            frame.unpin(),
            Err(BufferFrameError::AlreadyUnpinned)
        ));
    }

    #[test]
    fn page_mut은_dirty로_표시한다() {
        let mut frame = BufferFrame::new(page_key(0), Page::new());

        let _ = frame.page_mut();

        assert!(frame.is_dirty);
    }

    #[test]
    fn reference_bit은_pin에서_설정되고_unpin후에도_유지된다() {
        let mut frame = BufferFrame::new(page_key(0), Page::new());

        assert!(frame.is_referenced());

        frame.unreferenced();
        assert!(!frame.is_referenced());

        frame
            .unpin()
            .expect("초기 pin 상태의 frame은 unpin할 수 있어야 한다");
        assert!(!frame.is_referenced());

        frame.pin();
        assert!(frame.is_referenced());

        frame
            .unpin()
            .expect("다시 pin한 frame은 unpin할 수 있어야 한다");
        assert!(frame.is_referenced());
    }
}
