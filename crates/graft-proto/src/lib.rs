include!("mod.rs");

use std::{
    ops::{Bound, RangeBounds},
    time::SystemTime,
};

use bytes::Bytes;
use common::v1::{lsn_bound, LsnBound, LsnRange, Snapshot};
use graft_core::{gid::GidParseErr, lsn::LSN, VolumeId};
use prost_types::TimestampError;
use zerocopy::IntoBytes;

pub use graft::*;

impl Snapshot {
    pub fn new(vid: &VolumeId, lsn: LSN, last_offset: u32, timestamp: SystemTime) -> Self {
        Self {
            vid: Bytes::copy_from_slice(vid.as_bytes()),
            lsn,
            last_offset,
            timestamp: Some(timestamp.into()),
        }
    }

    pub fn vid(&self) -> Result<&VolumeId, GidParseErr> {
        self.vid.as_ref().try_into()
    }

    pub fn lsn(&self) -> LSN {
        self.lsn
    }

    pub fn last_offset(&self) -> u32 {
        self.last_offset
    }

    pub fn system_time(&self) -> Option<Result<SystemTime, TimestampError>> {
        self.timestamp.map(|ts| ts.try_into())
    }
}

impl LsnBound {
    fn as_bound(&self) -> Bound<&LSN> {
        match &self.bound {
            Some(lsn_bound::Bound::Included(lsn)) => Bound::Included(lsn),
            Some(lsn_bound::Bound::Excluded(lsn)) => Bound::Excluded(lsn),
            None => Bound::Unbounded,
        }
    }
}

impl From<Bound<&LSN>> for LsnBound {
    fn from(bound: Bound<&LSN>) -> Self {
        let bound = match bound {
            Bound::Included(lsn) => Some(lsn_bound::Bound::Included(*lsn)),
            Bound::Excluded(lsn) => Some(lsn_bound::Bound::Excluded(*lsn)),
            Bound::Unbounded => None,
        };
        Self { bound }
    }
}

impl RangeBounds<LSN> for LsnRange {
    fn start_bound(&self) -> Bound<&LSN> {
        self.start
            .as_ref()
            .map(|b| b.as_bound())
            .unwrap_or(Bound::Unbounded)
    }

    fn end_bound(&self) -> Bound<&LSN> {
        self.end
            .as_ref()
            .map(|b| b.as_bound())
            .unwrap_or(Bound::Unbounded)
    }
}

impl LsnRange {
    pub fn from_bounds<T: RangeBounds<LSN>>(bounds: T) -> Self {
        Self {
            start: Some(bounds.start_bound().into()),
            end: Some(bounds.end_bound().into()),
        }
    }

    pub fn start(&self) -> Option<LSN> {
        self.start.and_then(|b| match b.bound {
            Some(lsn_bound::Bound::Included(lsn)) => Some(lsn),
            Some(lsn_bound::Bound::Excluded(lsn)) => Some(lsn + 1),
            None => None,
        })
    }

    pub fn start_exclusive(&self) -> Option<LSN> {
        self.start.and_then(|b| match b.bound {
            Some(lsn_bound::Bound::Included(lsn)) => lsn.checked_sub(1),
            Some(lsn_bound::Bound::Excluded(lsn)) => Some(lsn),
            None => None,
        })
    }

    pub fn end(&self) -> Option<LSN> {
        self.end.and_then(|b| match b.bound {
            Some(lsn_bound::Bound::Included(lsn)) => Some(lsn),
            Some(lsn_bound::Bound::Excluded(lsn)) => Some(lsn.saturating_sub(1)),
            None => None,
        })
    }
}
