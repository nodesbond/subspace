use crate::sync::BundleSync;
use crate::{
    topic, BundleReceiver, GossipMessage, GossipMessageHandler, GossipValidator, LOG_TARGET,
};
use domain_bundles::{BundleDownloader, CompactBundlePool};
use futures::future::pending;
use futures::stream::FuturesUnordered;
use futures::{future, Future, FutureExt, StreamExt};
use parity_scale_codec::{Decode, Encode};
use parking_lot::Mutex;
use sc_network_gossip::GossipEngine;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::Block as BlockT;
use std::pin::Pin;
use std::sync::Arc;

type DownloadFuture<Extrinsic, Number, Hash, DomainHash> = dyn Future<Output = Result<Option<SignedBundle<Extrinsic, Number, Hash, DomainHash>>, String>>
    + Send;

/// A worker plays the executor gossip protocol.
pub struct GossipWorker<PBlock, Block, Executor, Pool, Extrinsic, Number, Hash, DomainHash>
where
    PBlock: BlockT,
    Block: BlockT,
    Executor: GossipMessageHandler<PBlock, Block>,
{
    gossip_validator: Arc<GossipValidator<PBlock, Block, Executor>>,
    gossip_engine: Arc<Mutex<GossipEngine<Block>>>,
    bundle_receiver: BundleReceiver,
    bundle_sync: Option<Arc<BundleSync<Pool, Extrinsic, Number, Hash, DomainHash>>>,
    pending_downloads:
        FuturesUnordered<Pin<Box<DownloadFuture<Extrinsic, Number, Hash, DomainHash>>>>,
}

impl<PBlock, Block, Executor, Pool, Extrinsic, Number, Hash, DomainHash>
    GossipWorker<PBlock, Block, Executor, Pool, Extrinsic, Number, Hash, DomainHash>
where
    PBlock: BlockT,
    Block: BlockT,
    Executor: GossipMessageHandler<PBlock, Block>,
    Pool: TransactionPool + 'static,
    Extrinsic: Send + Sync + 'static,
    Number: Send + Sync + 'static,
    Hash: Send + Sync + 'static,
    DomainHash: Send + Sync + 'static,
{
    pub(super) fn new(
        gossip_validator: Arc<GossipValidator<PBlock, Block, Executor>>,
        gossip_engine: Arc<Mutex<GossipEngine<Block>>>,
        bundle_receiver: BundleReceiver,
        bundle_sync: Option<(
            Arc<dyn CompactBundlePool<Pool, Number, Hash, DomainHash>>,
            Arc<dyn BundleDownloader<Extrinsic, Number, Hash, DomainHash>>,
        )>,
    ) -> Self {
        let ret = Self {
            gossip_validator,
            gossip_engine,
            bundle_receiver,
            bundle_sync: bundle_sync
                .map(|(pool, downloader)| Arc::new(BundleSync::new(pool, downloader))),
            pending_downloads: Default::default(),
        };
        ret.pending_downloads.push(Box::pin(pending::<
            Result<Option<SignedBundle<Extrinsic, Number, Hash, DomainHash>>, String>,
        >()));
        ret
    }

    fn gossip_bundle(&self, bundle_hash: SignedBundleHash) {
        let outgoing_message: GossipMessage<PBlock, Block> = GossipMessage::BundleHash(bundle_hash);
        let encoded_message = outgoing_message.encode();
        self.gossip_validator.note_rebroadcasted(&encoded_message);
        self.gossip_engine
            .lock()
            .gossip_message(topic::<Block>(), encoded_message, false);
    }

    fn on_download(
        &mut self,
        download_status: Result<Option<SignedBundle<Extrinsic, Number, Hash, DomainHash>>, String>,
    ) {
    }

    pub(super) async fn run(mut self) {
        let mut incoming = Box::pin(
            self.gossip_engine
                .lock()
                .messages_for(topic::<Block>())
                .filter_map(|notification| async move {
                    notification.sender.and_then(|sender| {
                        GossipMessage::<PBlock, Block>::decode(&mut &notification.message[..])
                            .ok()
                            .map(|msg| (sender, msg))
                    })
                }),
        );

        loop {
            let engine = self.gossip_engine.clone();
            let gossip_engine = future::poll_fn(|cx| engine.lock().poll_unpin(cx));

            futures::select! {
                gossip_message = incoming.next().fuse() => {
                    if let Some((sender, message)) = gossip_message {
                        tracing::debug!(target: LOG_TARGET, ?message, "Rebroadcasting an executor gossip message");
                        match message {
                            GossipMessage::Bundle(_) => todo!(),
                            GossipMessage::BundleHash(hash) => {
                                if let Some(sync) = &self.bundle_sync {
                                   let sync = sync.clone();
                                    self.pending_downloads.push(Box::pin(async move {
                                        sync.download(sender, hash).await
                                    }));
                                }
                            }
                        }
                    } else {
                        return
                    }
                }
                bundle_hash = self.bundle_receiver.next().fuse() => {
                    if let Some(bundle_hash) = bundle_hash {
                        self.gossip_bundle(bundle_hash);
                    }
                }
                download_status = self.pending_downloads.next().fuse() => {
                    download_status.map(|status| self.on_download(status));
                }
                _ = gossip_engine.fuse() => {
                    tracing::error!(target: LOG_TARGET, "Gossip engine has terminated.");
                    return;
                }
            }
        }
    }
}
