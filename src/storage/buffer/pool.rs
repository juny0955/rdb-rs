use thiserror::Error;

use crate::{
    storage::buffer::{
        frame::{BufferFrame, FrameId},
        page_key::PageKey,
        page_table::PageTable,
    },
    storage::page::Page,
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
    #[error("dirty 상태인 page 입니다")]
    PageDirty,
}

pub struct FrameGuard<'a> {
    frame: &'a mut BufferFrame,
}

impl<'a> FrameGuard<'a> {
    fn new(frame: &'a mut BufferFrame) -> Self {
        Self { frame }
    }

    pub fn page(&self) -> &Page {
        self.frame.page()
    }

    pub fn page_mut(&mut self) -> &mut Page {
        self.frame.page_mut()
    }
}

impl<'a> Drop for FrameGuard<'a> {
    fn drop(&mut self) {
        let _ = self.frame.unpin();
    }
}

#[derive(Debug)]
pub struct BufferPool {
    frames: Vec<Option<BufferFrame>>,
    page_table: PageTable,
    hand_index: usize,
}

impl BufferPool {
    pub fn new(capacity: usize) -> Self {
        let mut frames = Vec::new();
        frames.resize_with(capacity, || None);

        Self {
            frames,
            page_table: PageTable::new(),
            hand_index: 0,
        }
    }

    pub fn fetch_cached_page(
        &mut self,
        page_key: PageKey,
    ) -> Result<FrameGuard<'_>, BufferPoolError> {
        let frame = self
            .fetch_cached_frame(page_key)
            .ok_or(BufferPoolError::PageNotCached)?;
        Ok(FrameGuard::new(frame))
    }

    fn fetch_cached_frame(&mut self, page_key: PageKey) -> Option<&mut BufferFrame> {
        let frame_id = self.page_table.get(&page_key)?;
        let frame = self.frames.get_mut(frame_id.index())?.as_mut()?;
        frame.pin();
        Some(frame)
    }

    pub fn insert_page(
        &mut self,
        page_key: PageKey,
        page: Page,
    ) -> Result<FrameGuard<'_>, BufferPoolError> {
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

    pub fn remove_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        {
            let frame = self.get_frame_mut(frame_id)?;
            if frame.pin_count() > 0 {
                return Err(BufferPoolError::PagePinned);
            }
            if frame.is_dirty() {
                return Err(BufferPoolError::PageDirty);
            }
        }

        self.page_table
            .remove(&page_key)
            .ok_or(BufferPoolError::PageNotCached)?;
        self.frames
            .get_mut(frame_id.index())
            .and_then(Option::take)
            .ok_or(BufferPoolError::PageNotCached)?;

        Ok(())
    }

    pub fn select_clock_victim(&mut self) -> Result<Option<PageKey>, BufferPoolError> {
        let Some(frame_id) = self.select_clock_frame() else {
            return Ok(None);
        };

        let page_key = self.get_frame_mut(frame_id)?.page_key();
        Ok(Some(page_key))
    }

    fn select_clock_frame(&mut self) -> Option<FrameId> {
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
                frame.unreferenced();
                continue;
            }

            return Some(frame_id);
        }

        None
    }

    pub fn clean_frame(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;
        frame.mark_clean();
        Ok(())
    }

    pub fn contains_page(&self, page_key: PageKey) -> bool {
        self.page_table.get(&page_key).is_some()
    }

    pub fn is_dirty(&self, page_key: PageKey) -> Result<bool, BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame(frame_id)?;
        Ok(frame.is_dirty())
    }

    pub fn is_full(&self) -> bool {
        self.frames.iter().all(|frame| frame.is_some())
    }

    fn get_frame_id(&self, page_key: PageKey) -> Result<FrameId, BufferPoolError> {
        self.page_table
            .get(&page_key)
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn get_frame(&self, frame_id: FrameId) -> Result<&BufferFrame, BufferPoolError> {
        self.frames
            .get(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_ref()
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn get_frame_mut(&mut self, frame_id: FrameId) -> Result<&mut BufferFrame, BufferPoolError> {
        self.frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)
    }
}

#[cfg(test)]
mod tests;
