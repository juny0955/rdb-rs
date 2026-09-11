use super::*;

#[test]
fn clean_unpinned_page를_evict하면_frame을_재사용할_수_있다() {
    let page_key = key(0);
    let mut pool = BufferPool::new(1);
    let frame_id = pool
        .insert_frame(frame(0))
        .expect("빈 frame에 삽입해야 한다");
    unpin_frame(&mut pool, page_key);

    pool.remove_page(page_key)
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
        pool.remove_page(page_key),
        Err(BufferPoolError::PagePinned)
    ));
    assert!(pool.frames[frame_id.index()].is_some());
    assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
}
