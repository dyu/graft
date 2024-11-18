use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use object_store::ObjectStore;

use crate::volume::{catalog::VolumeCatalog, store::VolumeStore, updater::VolumeCatalogUpdater};

mod commit;
mod health;
mod pull_commits;
mod pull_offsets;
mod snapshot;

pub struct MetastoreApiState<O> {
    store: Arc<VolumeStore<O>>,
    catalog: VolumeCatalog,
    updater: VolumeCatalogUpdater,
}

impl<O> MetastoreApiState<O> {
    pub fn new(
        store: Arc<VolumeStore<O>>,
        catalog: VolumeCatalog,
        updater: VolumeCatalogUpdater,
    ) -> Self {
        Self { store, catalog, updater }
    }

    pub fn store(&self) -> &VolumeStore<O> {
        &self.store
    }

    pub fn catalog(&self) -> &VolumeCatalog {
        &self.catalog
    }

    pub fn updater(&self) -> &VolumeCatalogUpdater {
        &self.updater
    }
}

pub fn metastore_router<O>() -> Router<Arc<MetastoreApiState<O>>>
where
    O: ObjectStore + Sync + Send + 'static,
{
    Router::new()
        .route("/metastore/v1/health", get(health::handler))
        .route("/metastore/v1/snapshot", post(snapshot::handler))
        .route("/metastore/v1/pull_offsets", post(pull_offsets::handler))
        .route("/metastore/v1/pull_commits", post(pull_commits::handler))
        .route("/metastore/v1/commit", post(commit::handler))
}
