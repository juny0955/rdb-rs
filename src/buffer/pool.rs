use std::fs::File;

use thiserror::Error;

use crate::{
    buffer::{
        frame::{BufferFrame, FrameId},
        page_key::PageKey,
        page_table::PageTable,
    },
    page::{Page, PagerError, read_page, write_page},
};

#[derive(Debug, Error)]
pub enum BufferPoolError {
    #[error("빈 buffer frame이 없습니다")]
    NoFreeFrame,
    #[error("이미 buffer pool에 있는 page입니다")]
    PageAlreadyCached,
    #[error("이미 unpin된 page 입니다")]
    AlreadyUnpinned,
    #[error("cache 되지 않은 page 입니다")]
    PageNotCached,
    #[error("pin 상태인 page 입니다")]
    PagePinned,
    #[error(transparent)]
    Pager(#[from] PagerError),
}

pub(crate) struct FrameGuard<'a> {
    frame: &'a mut BufferFrame,
}

impl<'a> FrameGuard<'a> {
    fn new(frame: &'a mut BufferFrame) -> Self {
        Self { frame }
    }

    pub(crate) fn flush(&mut self, file: &mut File) -> Result<(), BufferPoolError> {
        if self.frame.is_dirty() {
            write_page(file, self.frame.page_key().page_id(), self.frame.page())?;
            self.frame.mark_clean();
        }

        Ok(())
    }

    pub(crate) fn page(&self) -> &Page {
        self.frame.page()
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        self.frame.page_mut()
    }
}

impl<'a> Drop for FrameGuard<'a> {
    fn drop(&mut self) {
        let _ = self.frame.unpin();
    }
}

#[derive(Debug)]
pub(crate) struct BufferPool {
    frames: Vec<Option<BufferFrame>>,
    page_table: PageTable,
    hand_index: usize,
}

impl BufferPool {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut frames = Vec::new();
        frames.resize_with(capacity, || None);
        Self {
            frames,
            page_table: PageTable::new(),
            hand_index: 0,
        }
    }

    pub(crate) fn fetch_page(
        &mut self,
        file: &mut File,
        page_key: PageKey,
    ) -> Result<FrameGuard<'_>, BufferPoolError> {
        if self.page_table.get(&page_key).is_some() {
            let frame = self
                .fetch_cached_frame(page_key)
                .ok_or(BufferPoolError::PageNotCached)?;

            return Ok(FrameGuard::new(frame));
        }

        if self.frames.iter().all(Option::is_some) && self.evict_clock_victim(file)?.is_none() {
            return Err(BufferPoolError::NoFreeFrame);
        }

        let page = read_page(file, page_key.page_id())?;
        let frame_id = self.insert_frame(BufferFrame::new(page_key, page))?;

        let frame = self.get_frame_mut(frame_id)?;
        Ok(FrameGuard::new(frame))
    }

    fn insert_frame(&mut self, frame: BufferFrame) -> Result<FrameId, BufferPoolError> {
        for (index, f) in self.frames.iter().enumerate() {
            if f.is_none() {
                let frame_id = FrameId::new(index);
                let page_key = frame.page_key();

                self.page_table
                    .insert(page_key, frame_id)
                    .map_err(|_| BufferPoolError::PageAlreadyCached)?;
                self.frames[index] = Some(frame);

                return Ok(frame_id);
            }
        }

        Err(BufferPoolError::NoFreeFrame)
    }

    fn fetch_cached_frame(&mut self, page_key: PageKey) -> Option<&mut BufferFrame> {
        let frame_id = self.page_table.get(&page_key)?;
        let frame = self.frames.get_mut(frame_id.index())?.as_mut()?;
        frame.pin();
        Some(frame)
    }

    fn unpin_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        frame.unpin().map_err(|_| BufferPoolError::AlreadyUnpinned)
    }

    pub(crate) fn flush_page(
        &mut self,
        file: &mut File,
        page_key: PageKey,
    ) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        if frame.is_dirty() {
            write_page(file, page_key.page_id(), frame.page())?;
            frame.mark_clean();
        }

        Ok(())
    }

    pub(crate) fn evict_clock_victim(
        &mut self,
        file: &mut File,
    ) -> Result<Option<PageKey>, BufferPoolError> {
        let Some(frame_id) = self.select_clock_victim() else {
            return Ok(None);
        };

        let page_key = self.get_frame_mut(frame_id)?.page_key();
        self.evict_page(file, page_key)?;
        Ok(Some(page_key))
    }

    fn evict_page(&mut self, file: &mut File, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        if frame.pin_count() > 0 {
            return Err(BufferPoolError::PagePinned);
        }

        if frame.is_dirty() {
            self.flush_page(file, page_key)?;
        }

        self.frames[frame_id.index()]
            .take()
            .ok_or(BufferPoolError::PageNotCached)?;
        self.page_table
            .remove(&page_key)
            .ok_or(BufferPoolError::PageNotCached)?;

        Ok(())
    }

    fn get_frame_id(&self, page_key: PageKey) -> Result<FrameId, BufferPoolError> {
        self.page_table
            .get(&page_key)
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn get_frame_mut(&mut self, frame_id: FrameId) -> Result<&mut BufferFrame, BufferPoolError> {
        self.frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn select_clock_victim(&mut self) -> Option<FrameId> {
        if self.frames.is_empty() {
            return None;
        }

        for _ in 0..(self.frames.len() * 2) {
            let index = self.hand_index;
            self.hand_index = (index + 1) % self.frames.len();

            let frame_id = FrameId::new(index);
            let Some(frame) = self.frames.get_mut(index).and_then(|slot| slot.as_mut()) else {
                continue;
            };

            if frame.pin_count() > 0 {
                continue;
            }

            if frame.is_referenced() {
                frame.unreference();
                continue;
            }

            return Some(frame_id);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Page, PageId, PagerError, Row, allocate_page, read_page};
    use crate::schema::TableId;
    use tempfile::NamedTempFile;

    fn key(page_id: u64) -> PageKey {
        PageKey::new(TableId::new(1), PageId::new(page_id))
    }

    fn page_key(page_id: PageId) -> PageKey {
        PageKey::new(TableId::new(1), page_id)
    }

    fn frame(page_id: u64) -> BufferFrame {
        BufferFrame::new(key(page_id), Page::new())
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
        let page_key = key(0);
        let mut pool = BufferPool::new(1);

        let frame_id = pool
            .insert_frame(BufferFrame::new(page_key, Page::new()))
            .expect("빈 frame에 삽입해야 한다");

        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn 중복_page_key_삽입은_slot을_변경하지_않는다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(BufferFrame::new(page_key, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(matches!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new())),
            Err(BufferPoolError::PageAlreadyCached)
        ));
        assert_eq!(pool.page_table.get(&page_key), Some(first_frame_id));
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn cache_hit이면_frame을_pin한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );

        let frame = pool
            .fetch_cached_frame(page_key)
            .expect("삽입한 page는 cache hit여야 한다");

        assert_eq!(frame.pin_count(), 2);
    }

    #[test]
    fn cache_miss는_pool_상태를_변경하지_않는다() {
        let cached_page_key = key(0);
        let missing_page_key = key(1);
        let mut pool = BufferPool::new(2);
        let frame_id = pool
            .insert_frame(BufferFrame::new(cached_page_key, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(pool.fetch_cached_frame(missing_page_key).is_none());
        assert_eq!(pool.page_table.get(&cached_page_key), Some(frame_id));
        assert!(pool.page_table.get(&missing_page_key).is_none());
        assert!(pool.frames[0].is_some());
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn unpin하면_pin_count가_감소한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );

        assert!(pool.unpin_page(page_key).is_ok());

        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
    }

    #[test]
    fn cache되지_않은_page를_unpin하면_오류다() {
        let mut pool = BufferPool::new(1);

        assert!(matches!(
            pool.unpin_page(key(0)),
            Err(BufferPoolError::PageNotCached)
        ));
    }

    #[test]
    fn 이미_unpin된_page를_다시_unpin하면_오류다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );
        assert!(pool.unpin_page(page_key).is_ok());

        assert!(matches!(
            pool.unpin_page(page_key),
            Err(BufferPoolError::AlreadyUnpinned)
        ));
        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
    }

    #[test]
    fn 수정한_frame은_unpin후에도_dirty다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );
        assert!(pool.unpin_page(page_key).is_ok());

        {
            let frame = pool
                .fetch_cached_frame(page_key)
                .expect("삽입한 page는 cache hit여야 한다");
            assert!(!frame.is_dirty());
            let _ = frame.page_mut();
            assert!(frame.is_dirty());
        }

        assert!(pool.unpin_page(page_key).is_ok());
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
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"flush");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        pool.insert_frame(frame)?;

        pool.flush_page(&mut file, page_key)?;

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
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let _ = frame.page_mut();
        let mut pool = BufferPool::new(1);
        assert!(pool.insert_frame(frame).is_ok());

        assert!(matches!(
            pool.flush_page(&mut file, page_key),
            Err(BufferPoolError::Pager(PagerError::PageNotAllocated(id))) if id == page_id
        ));
        assert!(
            pool.frames[0]
                .as_ref()
                .expect("frame이 있어야 한다")
                .is_dirty()
        );
    }

    #[test]
    fn clean_unpinned_page를_evict하면_frame을_재사용할_수_있다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        pool.unpin_page(page_key)
            .expect("처음 unpin은 성공해야 한다");

        pool.evict_page(&mut file, page_key)
            .expect("clean unpinned page는 제거해야 한다");

        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);
        assert_eq!(
            pool.insert_frame(frame(1))
                .expect("제거한 frame을 재사용해야 한다"),
            frame_id
        );
    }

    #[test]
    fn pinned_page는_evict하지_않고_상태를_유지한다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");

        assert!(matches!(
            pool.evict_page(&mut file, page_key),
            Err(BufferPoolError::PagePinned)
        ));
        assert!(pool.frames[frame_id.index()].is_some());
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn dirty_unpinned_page를_evict하면_disk에_기록한다() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"evict");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        let frame_id = pool.insert_frame(frame)?;
        pool.unpin_page(page_key)?;

        pool.evict_page(&mut file, page_key)?;

        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn dirty_page의_flush가_실패하면_evict하지_않는다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_id = PageId::new(0);
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let _ = frame.page_mut();
        let mut pool = BufferPool::new(1);
        let frame_id = pool.insert_frame(frame).expect("빈 frame에 삽입해야 한다");
        pool.unpin_page(page_key)
            .expect("처음 unpin은 성공해야 한다");

        assert!(matches!(
            pool.evict_page(&mut file, page_key),
            Err(BufferPoolError::Pager(PagerError::PageNotAllocated(id))) if id == page_id
        ));
        assert!(
            pool.frames[frame_id.index()]
                .as_ref()
                .expect("flush 실패 후 frame이 유지되어야 한다")
                .is_dirty()
        );
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn clock은_unpinned_reference_false_frame을_선택한다() {
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        pool.unpin_page(key(0)).expect("처음 unpin은 성공해야 한다");
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(frame_id));
    }

    #[test]
    fn clock은_reference_true_frame에_second_chance를_준다() {
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let second_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");
        pool.unpin_page(key(0)).expect("첫 frame을 unpin해야 한다");
        pool.unpin_page(key(1))
            .expect("두 번째 frame을 unpin해야 한다");
        pool.frames[second_frame_id.index()]
            .as_mut()
            .expect("두 번째 frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(second_frame_id));
        assert!(
            !pool.frames[first_frame_id.index()]
                .as_ref()
                .expect("첫 frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock은_pinned_frame을_건너뛴다() {
        let mut pool = BufferPool::new(2);
        let pinned_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let candidate_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");
        pool.unpin_page(key(1))
            .expect("두 번째 frame을 unpin해야 한다");
        pool.frames[candidate_frame_id.index()]
            .as_mut()
            .expect("후보 frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(candidate_frame_id));
        assert!(
            pool.frames[pinned_frame_id.index()]
                .as_ref()
                .expect("pinned frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock은_capacity_0이면_none을_반환한다() {
        let mut pool = BufferPool::new(0);

        assert_eq!(pool.select_clock_victim(), None);
    }

    #[test]
    fn clock은_모든_frame이_pinned이면_none을_반환한다() {
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let second_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");

        assert_eq!(pool.select_clock_victim(), None);
        assert!(
            pool.frames[first_frame_id.index()]
                .as_ref()
                .expect("첫 frame이 있어야 한다")
                .is_referenced()
        );
        assert!(
            pool.frames[second_frame_id.index()]
                .as_ref()
                .expect("두 번째 frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock_victim을_evict하면_page와_frame_매핑을_제거한다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        pool.unpin_page(page_key)
            .expect("처음 unpin은 성공해야 한다");
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert!(matches!(
            pool.evict_clock_victim(&mut file),
            Ok(Some(id)) if id == page_key
        ));
        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);
    }

    #[test]
    fn dirty_clock_victim을_evict하면_disk에_기록한다() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"clock evict");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        let frame_id = pool.insert_frame(frame)?;
        pool.unpin_page(page_key)?;
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.evict_clock_victim(&mut file)?, Some(page_key));
        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn clock_victim이_없으면_pool_상태를_유지한다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let mut file = test_file.reopen().expect("임시 파일을 열어야 한다");
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");

        assert!(matches!(pool.evict_clock_victim(&mut file), Ok(None)));
        assert!(pool.frames[frame_id.index()].is_some());
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }
}
