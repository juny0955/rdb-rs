use super::*;

#[test]
fn cache_hit_guard가_drop되면_기존_pin_count로_돌아온다() {
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    assert!(
        pool.insert_frame(BufferFrame::new(page_key, Page::new()))
            .is_ok()
    );

    {
        let _guard = pool
            .fetch_cached_frame(page_key)
            .map(FrameGuard::new)
            .expect("cache hit이어야 한다");
    }
    let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
    assert_eq!(frame.pin_count(), 1);
}

#[test]
fn 수정한_frame은_unpin후에도_dirty다() {
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    assert!(
        pool.insert_frame(BufferFrame::new(page_key, Page::new()))
            .is_ok()
    );
    unpin_frame(&mut pool, page_key);

    {
        let frame = pool
            .fetch_cached_frame(page_key)
            .expect("삽입한 page는 cache hit이어야 한다");
        assert!(!frame.is_dirty());
        let _ = frame.page_mut();
        assert!(frame.is_dirty());
    }

    unpin_frame(&mut pool, page_key);
    let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
    assert_eq!(frame.pin_count(), 0);
    assert!(frame.is_dirty());
}
