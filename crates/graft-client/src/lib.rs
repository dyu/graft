#![allow(clippy::module_inception)]

mod error;
mod metastore;
mod net;
pub mod oracle;
mod pagestore;
mod pair;

pub mod runtime {
    pub mod runtime;
    pub mod storage;
    pub mod sync;
    pub mod volume_handle;
    pub mod volume_reader;
    pub mod volume_writer;
}

pub use error::ClientErr;
pub use metastore::MetastoreClient;
pub use net::NetClient;
pub use pagestore::PagestoreClient;
pub use pair::ClientPair;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("graft-client/", env!("CARGO_PKG_VERSION"));
