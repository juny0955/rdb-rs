use std::path::PathBuf;

use thiserror::Error;

use crate::catalog::metadata::RelationId;
use crate::storage::file::{RelationFileManager, RelationFileManagerError};
use crate::storage::page::PageId;
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
    #[error(transparent)]
    RelationFileManager(#[from] RelationFileManagerError),
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
    relation_manager: RelationFileManager,
    hand_index: usize,
}

impl BufferPool {
    pub fn new(capacity: usize) -> Self {
        let mut frames = Vec::new();
        frames.resize_with(capacity, || None);

        Self {
            frames,
            page_table: PageTable::new(),
            relation_manager: RelationFileManager::new(),
            hand_index: 0,
        }
    }

    pub fn fetch_page(&mut self, page_key: PageKey) -> Result<FrameGuard<'_>, BufferPoolError> {
        if self.page_table.get(&page_key).is_some() {
            let frame = self
                .fetch_cached_frame(page_key)
                .ok_or(BufferPoolError::PageNotCached)?;

            return Ok(FrameGuard::new(frame));
        }

        if self.frames.iter().all(Option::is_some) && self.evict_clock_victim()?.is_none() {
            return Err(BufferPoolError::NoFreeFrame);
        }

        let page = self
            .relation_manager
            .read_page(page_key.relation_id(), page_key.page_id())?;
        let frame_id = self.insert_frame(BufferFrame::new(page_key, page))?;

        let frame = self.get_frame_mut(frame_id)?;
        Ok(FrameGuard::new(frame))
    }

    pub fn allocate_page(&mut self, relation_id: RelationId) -> Result<PageId, BufferPoolError> {
        Ok(self.relation_manager.allocate_page(relation_id)?)
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

    pub fn flush_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;

        let relation_manager = &self.relation_manager;
        let frames = &mut self.frames;

        let frame = frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)?;

        let page = frame.page();
        if frame.is_dirty() {
            relation_manager.write_page(page_key.relation_id(), page_key.page_id(), page)?;
            frame.mark_clean();
        }

        Ok(())
    }

    pub fn evict_clock_victim(&mut self) -> Result<Option<PageKey>, BufferPoolError> {
        let Some(frame_id) = self.select_clock_victim() else {
            return Ok(None);
        };

        let page_key = self.get_frame_mut(frame_id)?.page_key();
        self.evict_page(page_key)?;
        Ok(Some(page_key))
    }

    pub fn register_relation(&mut self, relation_id: RelationId, path: PathBuf) {
        self.relation_manager.register_relation(relation_id, path)
    }

    fn evict_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        if frame.pin_count() > 0 {
            return Err(BufferPoolError::PagePinned);
        }

        if frame.is_dirty() {
            self.flush_page(page_key)?;
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
                frame.unreferenced();
                continue;
            }

            return Some(frame_id);
        }

        None
    }
}

#[cfg(test)]
mod tests;
