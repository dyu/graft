use std::{fmt::Debug, mem, sync::Arc};

use bytes::BytesMut;
use culprit::{Culprit, Result, ResultExt};
use graft_client::{
    oracle::LeapOracle,
    runtime::{
        storage::snapshot::Snapshot,
        volume_handle::VolumeHandle,
        volume_reader::{VolumeRead, VolumeReader},
        volume_writer::{VolumeWrite, VolumeWriter},
    },
};
use graft_core::{
    PageIdx,
    page::{PAGESIZE, Page},
    page_count::PageCount,
};
use parking_lot::{Mutex, MutexGuard};
use sqlite_plugin::flags::{LockLevel, OpenOpts};

use crate::vfs::ErrCtx;

use super::VfsFile;

#[derive(Debug)]
enum VolFileState {
    Idle,
    Shared { reader: VolumeReader },
    Reserved { writer: VolumeWriter },
    Committing,
}

impl VolFileState {
    fn name(&self) -> &'static str {
        match self {
            VolFileState::Idle => "Idle",
            VolFileState::Shared { .. } => "Shared",
            VolFileState::Reserved { .. } => "Reserved",
            VolFileState::Committing => "Committing",
        }
    }
}

pub struct VolFile {
    handle: VolumeHandle,
    opts: OpenOpts,

    reserved: Arc<Mutex<()>>,
    state: VolFileState,
    oracle: Box<LeapOracle>,
}

impl Debug for VolFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.handle.vid().pretty())
    }
}

impl VolFile {
    pub fn new(handle: VolumeHandle, opts: OpenOpts, reserved: Arc<Mutex<()>>) -> Self {
        Self {
            handle,
            opts,
            reserved,
            state: VolFileState::Idle,
            oracle: Default::default(),
        }
    }

    pub fn snapshot_or_latest(&self) -> Result<Option<Snapshot>, ErrCtx> {
        match &self.state {
            VolFileState::Idle => self.handle().snapshot().or_into_ctx(),
            VolFileState::Shared { reader, .. } => Ok(reader.snapshot().cloned()),
            VolFileState::Reserved { writer, .. } => Ok(writer.snapshot().cloned()),
            VolFileState::Committing => ErrCtx::InvalidVolumeState.into(),
        }
    }

    pub fn handle(&self) -> &VolumeHandle {
        &self.handle
    }

    pub fn opts(&self) -> OpenOpts {
        self.opts
    }

    pub fn close(self) -> VolumeHandle {
        self.handle
    }
}

impl VfsFile for VolFile {
    fn readonly(&self) -> bool {
        false
    }

    fn in_memory(&self) -> bool {
        false
    }

    fn lock(&mut self, level: LockLevel) -> Result<(), ErrCtx> {
        match level {
            LockLevel::Unlocked => {
                // SQLite should never request an Unlocked lock
                unreachable!("bug: invalid request lock(Unlocked)");
            }
            LockLevel::Shared => {
                if let VolFileState::Idle = self.state {
                    // Transition Idle -> Shared
                    let reader = self.handle.reader().or_into_ctx()?;
                    self.state = VolFileState::Shared { reader };
                } else {
                    return Err(Culprit::new_with_note(
                        ErrCtx::InvalidLockTransition,
                        format!("invalid lock request Shared in state {}", self.state.name()),
                    ));
                }
            }
            LockLevel::Reserved => {
                if let VolFileState::Shared { ref reader } = self.state {
                    // Transition Shared -> Reserved

                    // Ensure that this VolFile is not readonly
                    if self.opts.mode().is_readonly() {
                        return Err(Culprit::new_with_note(
                            ErrCtx::InvalidLockTransition,
                            "invalid lock request: Shared -> Reserved: file is read-only",
                        ));
                    }

                    // try to acquire the reserved lock or fail if another thread has it
                    let Some(reserved) = self.reserved.try_lock() else {
                        return Err(Culprit::new(ErrCtx::Busy));
                    };

                    // upgrade the reader to a writer if possible
                    let latest_snapshot = self.handle.snapshot().or_into_ctx()?;

                    // check to see if the local LSN has changed since the transaction started.
                    // We can ignore checking the remote lsn because:
                    //  -> if the remote lsn changes due to a Pull, the local LSN will also change
                    //  -> if the remote lsn changes due to a Push, the logical state will not change
                    let writer = if reader.snapshot().map(|s| s.local())
                        != latest_snapshot.as_ref().map(|s| s.local())
                    {
                        // if a read occurred in this transaction, we can't
                        // upgrade to a reserved state
                        return Err(Culprit::new_with_note(
                            ErrCtx::BusySnapshot,
                            "unable to lock: Shared -> Reserved: snapshot changed",
                        ));
                    } else {
                        // The snapshot has not changed
                        self.handle.writer_at(latest_snapshot)
                    };

                    self.state = VolFileState::Reserved { writer };

                    // Explicitly leak the reserved lock
                    // SAFETY: we depend on SQLite to release the lock when it's done
                    MutexGuard::leak(reserved);
                } else {
                    return Err(Culprit::new_with_note(
                        ErrCtx::InvalidLockTransition,
                        format!(
                            "invalid lock request Reserved in state {}",
                            self.state.name()
                        ),
                    ));
                }
            }
            LockLevel::Pending | LockLevel::Exclusive => {
                // SQLite should only request these transitions while we are in the Reserved state
                assert!(
                    matches!(self.state, VolFileState::Reserved { .. }),
                    "bug: invalid lock request {:?} in state {}",
                    level,
                    self.state.name()
                );
            }
        }

        Ok(())
    }

    fn unlock(&mut self, level: LockLevel) -> Result<(), ErrCtx> {
        match level {
            LockLevel::Unlocked => match self.state {
                VolFileState::Idle | VolFileState::Shared { .. } | VolFileState::Committing => {
                    self.state = VolFileState::Idle;
                }
                VolFileState::Reserved { .. } => {
                    return Err(Culprit::new_with_note(
                        ErrCtx::InvalidLockTransition,
                        "invalid unlock request Unlocked in state Reserved",
                    ));
                }
            },
            LockLevel::Shared => {
                if let VolFileState::Reserved { writer } =
                    mem::replace(&mut self.state, VolFileState::Committing)
                {
                    // Transition Reserved -> Shared through the Committing state
                    // If we fail the commit, SQLite will subsequently issue an
                    // Unlocked request after handling the error

                    // Commit the writer, downgrading to a reader
                    let reader = writer.commit().or_into_ctx()?;
                    self.state = VolFileState::Shared { reader };

                    // release the reserved lock
                    // SAFETY: we are in the Reserved state, thus we are holding the lock
                    // SAFETY: we depend on the connection not being passed
                    // between threads while holding the lock
                    // TODO: find a way to assert that this thread actually owns the lock
                    assert!(self.reserved.is_locked(), "reserved lock must be locked");
                    unsafe { self.reserved.force_unlock() };
                } else {
                    return Err(Culprit::new_with_note(
                        ErrCtx::InvalidLockTransition,
                        format!(
                            "invalid unlock request Shared in state {}",
                            self.state.name()
                        ),
                    ));
                }
            }
            LockLevel::Reserved | LockLevel::Pending | LockLevel::Exclusive => {
                // SQLite should only request these transitions using the lock method
                unreachable!(
                    "bug: invalid unlock request {:?} in state {}",
                    level,
                    self.state.name()
                );
            }
        }

        Ok(())
    }

    fn file_size(&mut self) -> Result<usize, ErrCtx> {
        let pages = match &self.state {
            VolFileState::Idle => self
                .handle
                .snapshot()
                .or_into_ctx()?
                .map_or(PageCount::ZERO, |snapshot| snapshot.pages()),
            VolFileState::Shared { reader, .. } => {
                reader.snapshot().map_or(PageCount::ZERO, |s| s.pages())
            }
            VolFileState::Reserved { writer, .. } => writer.pages(),
            VolFileState::Committing => return ErrCtx::InvalidVolumeState.into(),
        };
        Ok((PAGESIZE * pages.to_usize()).as_usize())
    }

    fn read(&mut self, offset: usize, data: &mut [u8]) -> Result<usize, ErrCtx> {
        // locate the page offset of the requested page
        let page_idx: PageIdx = ((offset / PAGESIZE.as_usize()) + 1)
            .try_into()
            .expect("offset out of volume range");
        // local_offset is the offset *within* the requested page
        let local_offset = offset % PAGESIZE;

        assert!(
            local_offset + data.len() <= PAGESIZE,
            "read must not cross page boundary"
        );

        // load the page
        let page = match &mut self.state {
            VolFileState::Idle => {
                // sqlite sometimes reads the database header without holding a
                // lock, in this case we are expected to read from the latest
                // snapshot
                self.handle()
                    .reader()
                    .or_into_ctx()?
                    .read(self.oracle.as_mut(), page_idx)
                    .or_into_ctx()?
            }
            VolFileState::Shared { reader } => {
                reader.read(self.oracle.as_mut(), page_idx).or_into_ctx()?
            }
            VolFileState::Reserved { writer } => {
                writer.read(self.oracle.as_mut(), page_idx).or_into_ctx()?
            }
            VolFileState::Committing => return ErrCtx::InvalidVolumeState.into(),
        };

        let range = local_offset.as_usize()..(local_offset + data.len()).as_usize();
        data.copy_from_slice(&page[range]);
        Ok(data.len())
    }

    fn truncate(&mut self, size: usize) -> Result<(), ErrCtx> {
        let VolFileState::Reserved { writer, .. } = &mut self.state else {
            return Err(Culprit::new_with_note(
                ErrCtx::InvalidVolumeState,
                "must hold reserved lock to truncate",
            ));
        };

        assert_eq!(
            size % PAGESIZE.as_usize(),
            0,
            "size must be an even multiple of {PAGESIZE}"
        );

        let pages: PageCount = (size / PAGESIZE.as_usize())
            .try_into()
            .expect("size too large");

        writer.truncate(pages);
        Ok(())
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, ErrCtx> {
        let VolFileState::Reserved { writer, .. } = &mut self.state else {
            return Err(Culprit::new_with_note(
                ErrCtx::InvalidVolumeState,
                "must hold reserved lock to write",
            ));
        };

        // locate the requested page index
        let page_idx: PageIdx = ((offset / PAGESIZE.as_usize()) + 1)
            .try_into()
            .expect("offset out of volume range");
        // local_offset is the offset *within* the requested page
        let local_offset = offset % PAGESIZE;

        assert!(
            local_offset + data.len() <= PAGESIZE,
            "write must not cross page boundary"
        );

        let page = if data.len() == PAGESIZE {
            // writing a full page
            Page::try_from(data).expect("data is a full page")
        } else {
            // writing a partial page
            // we need to read and then update the page
            let mut page: BytesMut = writer
                .read(self.oracle.as_mut(), page_idx)
                .or_into_ctx()?
                .into();
            // SAFETY: we already verified that the write does not cross a page boundary
            let range = local_offset.as_usize()..(local_offset + data.len()).as_usize();
            page[range].copy_from_slice(data);
            page.try_into().expect("we did not change the page size")
        };

        writer.write(page_idx, page);
        Ok(data.len())
    }
}
