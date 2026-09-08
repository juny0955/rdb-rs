use super::*;

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
fn frame_guard_drop은_pin_count를_감소시킨다() {
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    let frame_id = pool
        .insert_frame(BufferFrame::new(page_key, Page::new()))
        .expect("frame을 삽입해야 한다");

    {
        let frame = pool.get_frame_mut(frame_id).expect("frame이 있어야 한다");
        let _guard = FrameGuard::new(frame);
    }

    let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
    assert_eq!(frame.pin_count(), 0);
}
