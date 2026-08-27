use crate::page::Page;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeType {
    Leaf,
    Internal,
}

impl NodeType {
    fn to_byte(&self) -> u8 {
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

pub(crate) struct BTreePageHeader {
    node_type: NodeType,
    entry_count: u16,
}

impl BTreePageHeader {
    pub(crate) fn new() -> Self {
        Self {
            node_type: NodeType::Leaf,
            entry_count: 0,
        }
    }

    pub(crate) fn to_bytes(&self) -> [u8; 3] {
        let mut bytes = [0u8; 3];
        bytes[0] = self.node_type.to_byte();
        bytes[1..3].copy_from_slice(&self.entry_count.to_be_bytes());
        bytes
    }

    pub(crate) fn from_bytes(bytes: [u8; 3]) -> Option<Self> {
        let node_type = NodeType::from_byte(bytes[0])?;
        let entry_count = u16::from_be_bytes([bytes[1], bytes[2]]);
        Some(Self {
            node_type,
            entry_count,
        })
    }

    pub(crate) fn write_to_page(&self, page: &mut Page) {
        let page_bytes = page.as_bytes_mut();
        page_bytes[0..3].copy_from_slice(&self.to_bytes());
    }

    pub(crate) fn read_from_page(page: &Page) -> Option<Self> {
        let page_bytes = page.as_bytes();
        Self::from_bytes([page_bytes[0], page_bytes[1], page_bytes[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 빈_leaf_header를_세_바이트로_직렬화한다() {
        // Given
        let header = BTreePageHeader::new();

        // When
        let bytes = header.to_bytes();

        // Then
        assert_eq!(bytes, [0, 0, 0]);
    }

    #[test]
    fn internal_header의_entry_count를_big_endian으로_직렬화한다() {
        // Given
        let header = BTreePageHeader {
            node_type: NodeType::Internal,
            entry_count: 258,
        };

        // When
        let bytes = header.to_bytes();

        // Then
        assert_eq!(bytes, [1, 1, 2]);
    }

    #[test]
    fn internal_header를_세_바이트에서_역직렬화한다() {
        // Given
        let bytes = [1, 1, 2];

        // When
        let header = BTreePageHeader::from_bytes(bytes);

        // Then
        assert!(matches!(
            header,
            Some(BTreePageHeader {
                node_type: NodeType::Internal,
                entry_count: 258,
            })
        ));
    }

    #[test]
    fn 알수없는_node_type은_역직렬화를_거부한다() {
        // Given
        let bytes = [2, 0, 0];

        // When
        let header = BTreePageHeader::from_bytes(bytes);

        // Then
        assert!(header.is_none());
    }
}
