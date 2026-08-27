use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BTreeKey {
    Int(i32),
    BigInt(i64),
    Boolean(bool),
    Varchar(String),
}

impl BTreeKey {
    pub(crate) fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (BTreeKey::Int(a), BTreeKey::Int(b)) => a.partial_cmp(b),
            (BTreeKey::BigInt(a), BTreeKey::BigInt(b)) => a.partial_cmp(b),
            (BTreeKey::Boolean(a), BTreeKey::Boolean(b)) => a.partial_cmp(b),
            (BTreeKey::Varchar(a), BTreeKey::Varchar(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}
