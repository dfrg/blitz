//! The keys a host names its nodes by.

/// A node of the host's document, as a backend holds it: opaque to the backend, which hands it
/// back when it is read and painted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeKey(pub u64);

impl NodeKey {
    /// Returns the key's value.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for NodeKey {
    #[inline]
    fn from(key: u64) -> Self {
        Self(key)
    }
}
