use crate::storage::page::{Page, PageId};

pub const LEAF_HEADER_SIZE: u16 = 14;
pub const INTERNAL_HEADER_SIZE: u16 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeType {
    Leaf,
    Internal,
}

impl NodeType {
    fn to_byte(self) -> u8 {
        match self {
            NodeType::Leaf => 0,
            NodeType::Internal => 1,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Leaf),
            1 => Some(Self::Internal),
            _ => None,
        }
    }
}

pub struct BTreePageHeader {
    node_type: NodeType,
    entry_count: u16,
    entry_end: u16,
    rightmost_child: Option<PageId>,
    next_leaf_page_id: Option<PageId>,
}

impl BTreePageHeader {
    pub fn new_leaf() -> Self {
        Self {
            node_type: NodeType::Leaf,
            entry_count: 0,
            entry_end: LEAF_HEADER_SIZE,
            rightmost_child: None,
            next_leaf_page_id: None,
        }
    }

    pub fn new_internal(rightmost_child: PageId) -> Self {
        Self {
            node_type: NodeType::Internal,
            entry_count: 0,
            entry_end: INTERNAL_HEADER_SIZE,
            rightmost_child: Some(rightmost_child),
            next_leaf_page_id: None,
        }
    }

    pub fn to_bytes(&self) -> [u8; 5] {
        let mut bytes = [0u8; 5];
        bytes[0] = self.node_type.to_byte();
        bytes[1..3].copy_from_slice(&self.entry_count.to_be_bytes());
        bytes[3..5].copy_from_slice(&self.entry_end.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 5 {
            return None;
        }

        let node_type = NodeType::from_byte(bytes[0])?;
        let entry_count = u16::from_be_bytes([bytes[1], bytes[2]]);
        let entry_end = u16::from_be_bytes([bytes[3], bytes[4]]);

        match node_type {
            NodeType::Leaf => {
                if entry_end < LEAF_HEADER_SIZE || entry_end as usize > bytes.len() {
                    return None;
                }

                let next_leaf_page_id = match bytes[5] {
                    0 => None,
                    1 => Some(PageId::from_bytes(bytes[6..14].try_into().ok()?)),
                    _ => return None,
                };

                Some(Self {
                    node_type,
                    entry_count,
                    entry_end,
                    rightmost_child: None,
                    next_leaf_page_id,
                })
            }
            NodeType::Internal => {
                if entry_end < INTERNAL_HEADER_SIZE
                    || bytes.len() < INTERNAL_HEADER_SIZE as usize
                    || entry_end as usize > bytes.len()
                {
                    return None;
                }

                let rightmost_child =
                    PageId::new(u64::from_be_bytes(bytes[5..13].try_into().ok()?));
                Some(Self {
                    node_type,
                    entry_count,
                    entry_end,
                    rightmost_child: Some(rightmost_child),
                    next_leaf_page_id: None,
                })
            }
        }
    }

    pub fn write_to_page(&self, page: &mut Page) {
        let page_bytes = page.as_bytes_mut();
        page_bytes[0..5].copy_from_slice(&self.to_bytes());

        match self.node_type {
            NodeType::Leaf => {
                page_bytes[5..LEAF_HEADER_SIZE as usize].fill(0);
                if let Some(next_leaf_page_id) = self.next_leaf_page_id {
                    page_bytes[5] = u8::from(true);
                    page_bytes[6..LEAF_HEADER_SIZE as usize]
                        .copy_from_slice(&next_leaf_page_id.to_bytes());
                } else {
                    page_bytes[5] = u8::from(false);
                }
            }
            NodeType::Internal => {
                if let Some(rightmost_child) = self.rightmost_child {
                    page_bytes[5..INTERNAL_HEADER_SIZE as usize]
                        .copy_from_slice(&rightmost_child.to_bytes());
                }
            }
        }
    }

    pub fn read_from_page(page: &Page) -> Option<Self> {
        let page_bytes = page.as_bytes();
        Self::from_bytes(page_bytes)
    }

    pub fn append_entry(&mut self, entry_len: u16) -> Option<()> {
        let next_count = self.entry_count.checked_add(1)?;
        let next_end = self.entry_end.checked_add(entry_len)?;

        self.entry_count = next_count;
        self.entry_end = next_end;
        Some(())
    }

    pub fn is_leaf(&self) -> bool {
        match &self.node_type {
            NodeType::Leaf => true,
            NodeType::Internal => false,
        }
    }

    pub fn entry_end(&self) -> u16 {
        self.entry_end
    }

    pub fn entry_count(&self) -> u16 {
        self.entry_count
    }

    pub fn rightmost_child(&self) -> Option<PageId> {
        self.rightmost_child
    }

    pub fn next_leaf_page_id(&self) -> Option<PageId> {
        self.next_leaf_page_id
    }

    pub fn set_next_leaf_page_id(&mut self, next: Option<PageId>) {
        self.next_leaf_page_id = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 다음_leaf가_없는_leaf_header를_저장하고_복원한다() {
        // Given
        let header = BTreePageHeader::new_leaf();
        let mut page = Page::new_raw();

        // When
        header.write_to_page(&mut page);
        let restored = BTreePageHeader::read_from_page(&page);

        // Then
        assert_eq!(
            &page.as_bytes()[0..14],
            &[0, 0, 0, 0, 14, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert!(matches!(restored, Some(ref header) if header.is_leaf()));
        assert_eq!(restored.and_then(|header| header.next_leaf_page_id()), None);
    }

    #[test]
    fn internal_header를_13바이트로_직렬화한다() {
        // Given
        let header = BTreePageHeader::new_internal(PageId::new(42));
        let mut page = Page::new_raw();

        // When
        header.write_to_page(&mut page);

        // Then
        assert_eq!(
            &page.as_bytes()[0..13],
            &[1, 0, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 42]
        );
    }

    #[test]
    fn internal_header를_page에서_복원한다() {
        // Given
        let header = BTreePageHeader::new_internal(PageId::new(42));
        let mut page = Page::new_raw();
        header.write_to_page(&mut page);

        // When
        let restored = BTreePageHeader::read_from_page(&page);

        // Then
        assert!(matches!(
            restored,
            Some(BTreePageHeader {
                node_type: NodeType::Internal,
                entry_count: 0,
                entry_end: 13,
                rightmost_child: Some(child),
                ..
            }) if child == PageId::new(42)
        ));
    }

    #[test]
    fn 다음_leaf_page_id를_저장하고_복원한다() {
        // Given
        let mut header = BTreePageHeader::new_leaf();
        header.set_next_leaf_page_id(Some(PageId::new(0)));
        let mut page = Page::new_raw();

        // When
        header.write_to_page(&mut page);
        let restored = BTreePageHeader::read_from_page(&page).expect("유효한 leaf header여야 함");

        // Then
        assert_eq!(restored.next_leaf_page_id(), Some(PageId::new(0)));
    }

    #[test]
    fn 알수없는_node_type은_역직렬화를_거부한다() {
        // Given
        let mut page = Page::new_raw();
        page.as_bytes_mut()[0] = 2;

        // When
        let header = BTreePageHeader::read_from_page(&page);

        // Then
        assert!(header.is_none());
    }
}
