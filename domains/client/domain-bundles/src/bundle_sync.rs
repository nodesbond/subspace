//! Bundle download from peers.

use crate::{Action, BundleDownloader, CompactBundlePool, GossipMessageHandler};
use cross_domain_message_gossip::{GossipMessageSink, Message};
use futures::future::pending;
use futures::stream::FuturesUnordered;
use futures::{Future, FutureExt, StreamExt};
use parking_lot::Mutex;
use sc_network::PeerId;
use sc_transaction_pool_api::TransactionPool;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_domains::{DomainId, SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use tracing::warn;

/// The result from the download future
struct DownloadResult<Extrinsic, Number, Hash, DomainHash> {
    bundle_hash: SignedBundleHash,
    result: Result<SignedBundle<Extrinsic, Number, Hash, DomainHash>, String>,
}
type DownloadFuture<Extrinsic, Number, Hash, DomainHash> =
    dyn Future<Output = DownloadResult<Extrinsic, Number, Hash, DomainHash>> + Send;

#[allow(clippy::type_complexity)]
pub struct BundleSync<Block, PBlock, Pool, Executor>
where
    Block: BlockT,
    PBlock: BlockT,
    Executor: GossipMessageHandler<PBlock, Block>,
{
    bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
    bundle_downloader: Arc<dyn BundleDownloader<Block, PBlock, Pool>>,
    receive_bundle_announcements: TracingUnboundedReceiver<(PeerId, SignedBundleHash)>,
    send_bundle_announcements: GossipMessageSink,
    executor: Executor,
    domain_id: DomainId,
    in_progress: Mutex<HashSet<SignedBundleHash>>,
    pending_downloads: FuturesUnordered<
        Pin<Box<DownloadFuture<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>>,
    >,
}

impl<Block, PBlock, Pool, Executor> BundleSync<Block, PBlock, Pool, Executor>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Executor: GossipMessageHandler<PBlock, Block>,
{
    pub fn new(
        bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
        bundle_downloader: Arc<dyn BundleDownloader<Block, PBlock, Pool>>,
        receive_bundle_announcements: TracingUnboundedReceiver<(PeerId, SignedBundleHash)>,
        send_bundle_announcements: GossipMessageSink,
        executor: Executor,
        domain_id: DomainId,
    ) -> Self {
        let ret = Self {
            bundle_pool,
            bundle_downloader,
            receive_bundle_announcements,
            send_bundle_announcements,
            executor,
            domain_id,
            in_progress: Mutex::new(HashSet::new()),
            pending_downloads: Default::default(),
        };
        ret.pending_downloads.push(Box::pin(pending::<
            DownloadResult<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
        >()));
        ret
    }

    /// The worker task
    pub async fn run(mut self) {
        loop {
            futures::select! {
                msg = self.receive_bundle_announcements.next().fuse() => {
                    if let Some((sender, bundle_hash)) = msg {
                        self.on_announcement(sender, bundle_hash);
                    }
                }
                status = self.pending_downloads.next().fuse() => {
                    if let Some(download_status) = status {
                        self.on_download_completion(download_status);
                    }
                }
            }
        }
    }

    /// Handles the bundle announcement received from the network
    fn on_announcement(&mut self, sender: PeerId, bundle_hash: SignedBundleHash) {
        tracing::info!("xxx: bundle sync announcement: {sender:?}, {bundle_hash:?}");
        if self.bundle_pool.contains(&bundle_hash) {
            return;
        }

        {
            let mut in_progress = self.in_progress.lock();
            if in_progress.contains(&bundle_hash) {
                return;
            }
            in_progress.insert(bundle_hash);
        }

        let downloader = self.bundle_downloader.clone();
        self.pending_downloads.push(Box::pin(async move {
            DownloadResult {
                bundle_hash,
                result: downloader.download_bundle(sender, bundle_hash).await,
            }
        }));
    }

    /// Handles a bundle download completion
    fn on_download_completion(
        &mut self,
        download_status: DownloadResult<
            Block::Extrinsic,
            NumberFor<PBlock>,
            PBlock::Hash,
            Block::Hash,
        >,
    ) {
        tracing::info!(
            "xxx: bundle sync download completion: {:?}, err = {}",
            download_status.bundle_hash,
            download_status.result.is_err()
        );
        self.in_progress.lock().remove(&download_status.bundle_hash);
        let bundle = match download_status.result {
            Ok(bundle) => bundle,
            Err(err) => {
                warn!(
                    "xxx: bundle download failed: {err:?}, {:?}",
                    download_status.bundle_hash
                );
                return;
            }
        };

        let ret = self.executor.on_bundle(&bundle);
        match ret {
            Ok(Action::RebroadcastBundle) => {
                tracing::info!(
                    "xxx: bundle sync download completion: {:?}, rebroadcasting",
                    download_status.bundle_hash
                );
                // Update the pool, announce to peers.
                self.bundle_pool.add(&bundle);

                let msg = Message {
                    domain_id: self.domain_id,
                    payload: download_status.bundle_hash.into(),
                };
                if let Err(err) = self.send_bundle_announcements.unbounded_send(msg) {
                    warn!("xxx: bundle download: error announcing downloaded bundle: {err:?}");
                }
            }
            _ => {
                warn!(
                    "xxx: bundle sync download:validation failed {:?}, {:?}",
                    ret, download_status.bundle_hash
                );
            }
        }
    }
}
