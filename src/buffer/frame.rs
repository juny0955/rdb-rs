use crate::page::{Page, PageId};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct FrameId(usize);

impl FrameId {
    pub(crate) fn new(id: usize) -> Self {
        Self(id)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BufferFrameError {
    AlreadyUnpinned,
}

pub struct BufferFrame {
    page_id: PageId,
    page: Page,
    pin_count: usize,
    is_dirty: bool,
}

impl BufferFrame {
    pub fn new(page_id: PageId, page: Page) -> Self {
        Self {
            page_id,
            page,
            pin_count: 1,
            is_dirty: false,
        }
    }

    pub fn pin(&mut self) {
        self.pin_count += 1;
    }

    pub fn unpin(&mut self) -> Result<(), BufferFrameError> {
        if self.pin_count == 0 {
            return Err(BufferFrameError::AlreadyUnpinned);
        }

        self.pin_count -= 1;
        Ok(())
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
}
