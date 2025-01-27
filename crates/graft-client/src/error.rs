use std::fmt::Debug;

use graft_proto::common::v1::{GraftErr, GraftErrCode};
use thiserror::Error;

use crate::runtime::storage;

#[derive(Error, Debug)]
pub enum ClientErr {
    #[error("graft error: {0}")]
    GraftErr(#[from] GraftErr),

    #[error("http request failed: {0}")]
    HttpErr(ureq::ErrorKind),

    #[error("failed to decode protobuf message")]
    ProtobufDecodeErr,

    #[error("failed to parse splinter: {0}")]
    SplinterParseErr(#[from] splinter::DecodeErr),

    #[error("local storage error: {0}")]
    StorageErr(#[from] storage::StorageErr),

    #[error("io error: {0}")]
    IoErr(std::io::ErrorKind),
}

impl From<std::io::Error> for ClientErr {
    fn from(err: std::io::Error) -> Self {
        Self::IoErr(err.kind())
    }
}

impl From<ureq::Error> for ClientErr {
    fn from(err: ureq::Error) -> Self {
        Self::HttpErr(err.kind())
    }
}

impl From<ureq::Transport> for ClientErr {
    fn from(err: ureq::Transport) -> Self {
        Self::HttpErr(err.kind())
    }
}

impl From<prost::DecodeError> for ClientErr {
    fn from(_: prost::DecodeError) -> Self {
        ClientErr::ProtobufDecodeErr
    }
}

impl ClientErr {
    pub(crate) fn is_snapshot_missing(&self) -> bool {
        match self {
            Self::GraftErr(err) => err.code() == GraftErrCode::SnapshotMissing,
            _ => false,
        }
    }

    pub(crate) fn is_commit_rejected(&self) -> bool {
        match self {
            Self::GraftErr(err) => err.code() == GraftErrCode::CommitRejected,
            _ => false,
        }
    }
}
