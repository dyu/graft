use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

#[derive(
    Debug, KnownLayout, Immutable, TryFromBytes, IntoBytes, Unaligned, Clone, Copy, PartialEq, Eq,
)]
#[repr(u8)]
pub enum SyncDirection {
    Push = 1,
    Pull = 2,
    Both = 3,
}

impl SyncDirection {
    pub fn matches(self, other: SyncDirection) -> bool {
        match (self, other) {
            (SyncDirection::Both, _) => true,
            (_, SyncDirection::Both) => true,
            (a, b) => a == b,
        }
    }
}

#[derive(KnownLayout, Immutable, TryFromBytes, IntoBytes, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct VolumeConfig {
    sync: SyncDirection,
}

impl VolumeConfig {
    pub fn new(sync: SyncDirection) -> Self {
        Self { sync }
    }

    pub fn sync(&self) -> SyncDirection {
        self.sync
    }
}

impl AsRef<[u8]> for VolumeConfig {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
