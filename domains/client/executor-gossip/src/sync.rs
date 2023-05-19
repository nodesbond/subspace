//! Syncing of bundles from peers.

use domain_bundles::{BundleDownloader, CompactBundlePool};
use parking_lot::Mutex;
use sc_network::PeerId;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct BundleSync<Block, PBlock, Pool> {
    bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
    bundle_downloader: Arc<dyn BundleDownloader<Block, PBlock, Pool>>,
    in_progress: Mutex<HashSet<SignedBundleHash>>,
}

impl<Block, PBlock, Pool> BundleSync<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    pub(crate) fn new(
        bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
        bundle_downloader: Arc<dyn BundleDownloader<Block, PBlock, Pool>>,
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
    ) -> Result<
        Option<SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>,
        String,
    > {
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
