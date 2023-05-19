//! Syncing of bundles from peers.

use domain_bundles::{BundleDownloader, CompactBundlePool};
use parking_lot::Mutex;
use sc_network::PeerId;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct BundleSync<Pool, Extrinsic, Number, Hash, DomainHash> {
    bundle_pool: Arc<dyn CompactBundlePool<Pool, Number, Hash, DomainHash>>,
    bundle_downloader: Arc<dyn BundleDownloader<Extrinsic, Number, Hash, DomainHash>>,
    in_progress: Mutex<HashSet<SignedBundleHash>>,
}

impl<Pool, Extrinsic, Number, Hash, DomainHash>
    BundleSync<Pool, Extrinsic, Number, Hash, DomainHash>
where
    Pool: TransactionPool,
    Extrinsic: Send + Sync,
    Number: Send + Sync,
    Hash: Send + Sync,
    DomainHash: Send + Sync,
{
    pub(crate) fn new(
        bundle_pool: Arc<dyn CompactBundlePool<Pool, Number, Hash, DomainHash>>,
        bundle_downloader: Arc<dyn BundleDownloader<Extrinsic, Number, Hash, DomainHash>>,
    ) -> Self {
        Self {
            bundle_pool,
            bundle_downloader,
            in_progress: Mutex::new(HashSet::new()),
        }
    }

    /// Downloads the bundle specified by the hash
    pub(crate) async fn download(
        &self,
        peer: PeerId,
        hash: SignedBundleHash,
    ) -> Result<Option<SignedBundle<Extrinsic, Number, Hash, DomainHash>>, String> {
        if self.bundle_pool.contains(&hash) {
            return Ok(None);
        }

        {
            let mut in_progress = self.in_progress.lock();
            if in_progress.contains(&hash) {
                return Ok(None);
            }
            in_progress.insert(hash.clone());
        }

        let ret = self
            .bundle_downloader
            .download_bundle(peer, hash.clone())
            .await;
        self.in_progress.lock().remove(&hash);

        ret.map(|bundle| Some(bundle))
    }
}
