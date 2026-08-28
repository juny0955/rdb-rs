use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeKeyType {
    Int,
    BigInt,
    Boolean,
    Varchar,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BTreeKey {
    Int(i32),
    BigInt(i64),
    Boolean(bool),
    Varchar(String),
}

impl BTreeKey {
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (BTreeKey::Int(a), BTreeKey::Int(b)) => a.partial_cmp(b),
            (BTreeKey::BigInt(a), BTreeKey::BigInt(b)) => a.partial_cmp(b),
            (BTreeKey::Boolean(a), BTreeKey::Boolean(b)) => a.partial_cmp(b),
            (BTreeKey::Varchar(a), BTreeKey::Varchar(b)) => a.partial_cmp(b),
            _ => None,
        }
    }

    pub fn key_type(&self) -> BTreeKeyType {
        match self {
            BTreeKey::Int(_) => BTreeKeyType::Int,
            BTreeKey::BigInt(_) => BTreeKeyType::BigInt,
            BTreeKey::Boolean(_) => BTreeKeyType::Boolean,
            BTreeKey::Varchar(_) => BTreeKeyType::Varchar,
        }
    }
}
