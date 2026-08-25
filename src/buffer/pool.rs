use std::fs::File;

use thiserror::Error;

use crate::{
    buffer::{
        frame::{BufferFrame, FrameId},
        page_table::PageTable,
    },
    page::{PageId, PagerError, write_page},
};

#[derive(Debug, Error)]
pub(crate) enum BufferPoolError {
    #[error("빈 buffer frame이 없습니다")]
    NoFreeFrame,
    #[error("이미 buffer pool에 있는 page입니다")]
    PageAlreadyCached,
    #[error("이미 unpin된 page 입니다")]
    AlreadyUnpinned,
    #[error("cache 되지 않은 page 입니다")]
    PageNotCached,
    #[error(transparent)]
    Pager(#[from] PagerError),
}

pub(crate) struct BufferPool {
    frames: Vec<Option<BufferFrame>>,
    page_table: PageTable,
}

impl BufferPool {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut frames = Vec::new();
        frames.resize_with(capacity, || None);
        Self {
            frames,
            page_table: PageTable::new(),
        }
    }

    pub(super) fn insert_frame(&mut self, frame: BufferFrame) -> Result<FrameId, BufferPoolError> {
        for (index, f) in self.frames.iter().enumerate() {
            if f.is_none() {
                let frame_id = FrameId::new(index);
                let page_id = frame.page_id();

                self.page_table
                    .insert(page_id, frame_id)
                    .map_err(|_| BufferPoolError::PageAlreadyCached)?;
                self.frames[index] = Some(frame);

                return Ok(frame_id);
            }
        }

        Err(BufferPoolError::NoFreeFrame)
    }

    pub(super) fn fetch_cached_frame(&mut self, page_id: PageId) -> Option<&mut BufferFrame> {
        let frame_id = self.page_table.get(&page_id)?;
        let frame = self.frames.get_mut(frame_id.index())?.as_mut()?;
        frame.pin();
        Some(frame)
    }

    pub(crate) fn unpin_page(&mut self, page_id: PageId) -> Result<(), BufferPoolError> {
        let frame_id = self
            .page_table
            .get(&page_id)
            .ok_or(BufferPoolError::PageNotCached)?;

        let frame = self
            .frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)?;

        frame.unpin().map_err(|_| BufferPoolError::AlreadyUnpinned)
    }

    pub(crate) fn flush_page(
        &mut self,
        file: &mut File,
        page_id: PageId,
    ) -> Result<(), BufferPoolError> {
        let frame_id = self
            .page_table
            .get(&page_id)
            .ok_or(BufferPoolError::PageNotCached)?;

        let frame = self
            .frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)?;

        if frame.is_dirty() {
            write_page(file, page_id, frame.page())?;
            frame.mark_clean();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Page, PageId, Row, allocate_page, read_page};
    use tempfile::NamedTempFile;

    fn frame(page_id: u64) -> BufferFrame {
        BufferFrame::new(PageId::new(page_id), Page::new())
    }

    #[test]
    fn capacity만큼_빈_frame을_생성한다() {
        let pool = BufferPool::new(2);

        assert_eq!(pool.frames.len(), 2);
        assert!(pool.frames.iter().all(|frame| frame.is_none()));
    }

    #[test]
    fn 빈_frame에_순서대로_삽입한다() {
        let mut pool = BufferPool::new(2);

        assert!(matches!(
            pool.insert_frame(frame(0)),
            Ok(frame_id) if frame_id == FrameId::new(0)
        ));
        assert!(matches!(
            pool.insert_frame(frame(1)),
            Ok(frame_id) if frame_id == FrameId::new(1)
        ));
    }

    #[test]
    fn 삽입한_frame을_page_table에서_조회한다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(1);

        let frame_id = pool
            .insert_frame(BufferFrame::new(page_id, Page::new()))
            .expect("빈 frame에 삽입해야 한다");

        assert_eq!(pool.page_table.get(&page_id), Some(frame_id));
    }

    #[test]
    fn 중복_page_id_삽입은_slot을_변경하지_않는다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(BufferFrame::new(page_id, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(matches!(
            pool.insert_frame(BufferFrame::new(page_id, Page::new())),
            Err(BufferPoolError::PageAlreadyCached)
        ));
        assert_eq!(pool.page_table.get(&page_id), Some(first_frame_id));
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn cache_hit이면_frame을_pin한다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_id, Page::new()))
                .is_ok()
        );

        let frame = pool
            .fetch_cached_frame(page_id)
            .expect("삽입한 page는 cache hit여야 한다");

        assert_eq!(frame.pin_count(), 2);
    }

    #[test]
    fn cache_miss는_pool_상태를_변경하지_않는다() {
        let cached_page_id = PageId::new(0);
        let missing_page_id = PageId::new(1);
        let mut pool = BufferPool::new(2);
        let frame_id = pool
            .insert_frame(BufferFrame::new(cached_page_id, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(pool.fetch_cached_frame(missing_page_id).is_none());
        assert_eq!(pool.page_table.get(&cached_page_id), Some(frame_id));
        assert!(pool.page_table.get(&missing_page_id).is_none());
        assert!(pool.frames[0].is_some());
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn unpin하면_pin_count가_감소한다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_id, Page::new()))
                .is_ok()
        );

        assert!(pool.unpin_page(page_id).is_ok());

        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
    }

    #[test]
    fn cache되지_않은_page를_unpin하면_오류다() {
        let mut pool = BufferPool::new(1);

        assert!(matches!(
            pool.unpin_page(PageId::new(0)),
            Err(BufferPoolError::PageNotCached)
        ));
    }

    #[test]
    fn 이미_unpin된_page를_다시_unpin하면_오류다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_id, Page::new()))
                .is_ok()
        );
        assert!(pool.unpin_page(page_id).is_ok());

        assert!(matches!(
            pool.unpin_page(page_id),
            Err(BufferPoolError::AlreadyUnpinned)
        ));
        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
    }

    #[test]
    fn 수정한_frame은_unpin후에도_dirty다() {
        let page_id = PageId::new(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_id, Page::new()))
                .is_ok()
        );
        assert!(pool.unpin_page(page_id).is_ok());

        {
            let frame = pool
                .fetch_cached_frame(page_id)
                .expect("삽입한 page는 cache hit여야 한다");
            assert!(!frame.is_dirty());
            let _ = frame.page_mut();
            assert!(frame.is_dirty());
        }

        assert!(pool.unpin_page(page_id).is_ok());
        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
        assert!(frame.is_dirty());
    }

    #[test]
    fn 가득찬_pool에_삽입하면_기존_frame을_유지한다() {
        let mut pool = BufferPool::new(1);
        assert!(pool.insert_frame(frame(0)).is_ok());

        assert!(matches!(
            pool.insert_frame(frame(1)),
            Err(BufferPoolError::NoFreeFrame)
        ));
        assert!(pool.frames[0].is_some());
    }

    #[test]
    fn dirty_frame을_flush하면_disk에_기록하고_clean으로_표시한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let mut frame = BufferFrame::new(page_id, Page::new());
        let row = Row::from_bytes(b"flush");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        pool.insert_frame(frame)?;

        pool.flush_page(&mut file, page_id)?;

        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert!(!frame.is_dirty());
        assert_eq!(frame.pin_count(), 1);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn flush가_실패하면_dirty_상태를_유지한다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_id = PageId::new(0);
        let mut frame = BufferFrame::new(page_id, Page::new());
        let _ = frame.page_mut();
        let mut pool = BufferPool::new(1);
        assert!(pool.insert_frame(frame).is_ok());

        assert!(matches!(
            pool.flush_page(&mut file, page_id),
            Err(BufferPoolError::Pager(PagerError::PageNotAllocated(id))) if id == page_id
        ));
        assert!(
            pool.frames[0]
                .as_ref()
                .expect("frame이 있어야 한다")
                .is_dirty()
        );
    }
}
