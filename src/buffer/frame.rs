use crate::page::{Page, PageId};

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
    page_id: PageId,
    page: Page,
    pin_count: usize,
    is_dirty: bool,
    referenced: bool,
}

impl BufferFrame {
    pub(super) fn new(page_id: PageId, page: Page) -> Self {
        Self {
            page_id,
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

    pub(super) fn unreference(&mut self) {
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

    pub(super) fn page_id(&self) -> PageId {
        self.page_id
    }

    pub(super) fn page(&self) -> &Page {
        &self.page
    }

    pub(super) fn page_mut(&mut self) -> &mut Page {
        self.is_dirty = true;
        &mut self.page
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_unpin_테스트() {
        let mut frame = BufferFrame::new(PageId::new(0), Page::new());

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
        let mut frame = BufferFrame::new(PageId::new(0), Page::new());

        let _ = frame.page_mut();

        assert!(frame.is_dirty);
    }

    #[test]
    fn reference_bit은_pin에서_설정되고_unpin후에도_유지된다() {
        let mut frame = BufferFrame::new(PageId::new(0), Page::new());

        assert!(frame.is_referenced());

        frame.unreference();
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
