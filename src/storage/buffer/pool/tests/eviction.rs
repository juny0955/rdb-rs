use super::*;

#[test]
fn clean_unpinned_page를_evict하면_frame을_재사용할_수_있다() {
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    let frame_id = pool
        .insert_frame(frame(0))
        .expect("빈 frame에 삽입해야 한다");
    unpin_frame(&mut pool, page_key);

    pool.evict_page(page_key)
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
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    let frame_id = pool
        .insert_frame(frame(0))
        .expect("빈 frame에 삽입해야 한다");

    assert!(matches!(
        pool.evict_page(page_key),
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
    register_test_file(&mut pool, &test_file);
    let frame_id = pool.insert_frame(frame)?;
    unpin_frame(&mut pool, page_key);

    pool.evict_page(page_key)?;

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
    let page_id = PageId::new(0);
    let page_key = page_key(page_id);
    let mut frame = BufferFrame::new(page_key, Page::new());
    let _ = frame.page_mut();
    let mut pool = BufferPool::new(1);
    register_test_file(&mut pool, &test_file);
    let frame_id = pool.insert_frame(frame).expect("빈 frame에 삽입해야 한다");
    unpin_frame(&mut pool, page_key);

    assert!(matches!(
        pool.evict_page(page_key),
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
