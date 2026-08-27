#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
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

pub struct BTreePageHeader {
    node_type: NodeType,
    entry_count: u16,
}

impl BTreePageHeader {
    pub fn new() -> Self {
        Self {
            node_type: NodeType::Leaf,
            entry_count: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 3] {
        let mut bytes = [0u8; 3];
        bytes[0] = self.node_type.to_byte();
        bytes[1..3].copy_from_slice(&self.entry_count.to_be_bytes());
        bytes
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
}
