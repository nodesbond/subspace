use futures::{FutureExt, StreamExt};
use parity_scale_codec::{Decode, Encode};
use parking_lot::{Mutex, RwLock};
use sc_network::config::NonDefaultSetConfig;
use sc_network::PeerId;
use sc_network_gossip::{
    GossipEngine, MessageIntent, Syncing as GossipSyncing, ValidationResult, Validator,
    ValidatorContext,
};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_core::twox_256;
use sp_domains::{DomainId, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, Hash as HashT, Header as HeaderT};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

const LOG_TARGET: &str = "cross_domain_gossip_worker";
const PROTOCOL_NAME: &str = "/subspace/cross-domain-messages";

/// Unbounded sender to send encoded ext to listeners.
pub type DomainTxPoolSink = TracingUnboundedSender<Vec<u8>>;
type MessageHash = [u8; 32];

/// Unbounded sender for bundle announcements.
pub type DomainBundleAnnouncementSink = TracingUnboundedSender<(PeerId, SignedBundleHash)>;

/// A cross domain message with encoded data.
#[derive(Debug, Encode, Decode)]
pub struct Message {
    pub domain_id: DomainId,
    pub payload: MessagePayload,
}

#[derive(Debug, Encode, Decode)]
pub enum MessagePayload {
    EncodedData(Vec<u8>),
    BundleAnnouncement(SignedBundleHash),
}

impl From<Vec<u8>> for MessagePayload {
    fn from(encoded_data: Vec<u8>) -> MessagePayload {
        MessagePayload::EncodedData(encoded_data)
    }
}

impl From<SignedBundleHash> for MessagePayload {
    fn from(bundle_hash: SignedBundleHash) -> MessagePayload {
        MessagePayload::BundleAnnouncement(bundle_hash)
    }
}

#[derive(Debug)]
pub enum MessageSource {
    Local,
    Network(Option<PeerId>),
}

/// Gossip worker builder
pub struct GossipWorkerBuilder {
    gossip_msg_stream: TracingUnboundedReceiver<Message>,
    gossip_msg_sink: TracingUnboundedSender<Message>,
    domain_tx_pool_sinks: BTreeMap<DomainId, DomainTxPoolSink>,
    domain_bundle_announcement_sinks: BTreeMap<DomainId, DomainBundleAnnouncementSink>,
}

impl GossipWorkerBuilder {
    /// Construct a gossip worker builder
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (gossip_msg_sink, gossip_msg_stream) =
            tracing_unbounded("cross_domain_gossip_messages", 100);
        Self {
            gossip_msg_stream,
            gossip_msg_sink,
            domain_tx_pool_sinks: BTreeMap::new(),
            domain_bundle_announcement_sinks: BTreeMap::new(),
        }
    }

    /// Collect the domain tx pool sink that will be used by the gossip message worker later.
    pub fn push_domain_tx_pool_sink(
        &mut self,
        domain_id: DomainId,
        tx_pool_sink: DomainTxPoolSink,
    ) {
        self.domain_tx_pool_sinks.insert(domain_id, tx_pool_sink);
    }

    /// Collect the domain bundle announcement sinks that will be used by the gossip
    /// message worker later.
    pub fn push_domain_bundle_announcement_sink(
        &mut self,
        domain_id: DomainId,
        bundle_announcement_sink: DomainBundleAnnouncementSink,
    ) {
        self.domain_bundle_announcement_sinks
            .insert(domain_id, bundle_announcement_sink);
    }

    /// Get the gossip message sink
    pub fn gossip_msg_sink(&self) -> TracingUnboundedSender<Message> {
        self.gossip_msg_sink.clone()
    }

    /// Build gossip worker
    pub fn build<Block, Network, GossipSync>(
        self,
        network: Network,
        sync: Arc<GossipSync>,
    ) -> GossipWorker<Block>
    where
        Block: BlockT,
        Network: sc_network_gossip::Network<Block> + Send + Sync + Clone + 'static,
        GossipSync: GossipSyncing<Block> + 'static,
    {
        let Self {
            gossip_msg_stream,
            domain_tx_pool_sinks,
            domain_bundle_announcement_sinks,
            ..
        } = self;

        let gossip_validator = Arc::new(GossipValidator::default());
        let gossip_engine = Arc::new(Mutex::new(GossipEngine::new(
            network,
            sync,
            PROTOCOL_NAME,
            gossip_validator.clone(),
            None,
        )));

        GossipWorker {
            gossip_engine,
            gossip_validator,
            gossip_msg_stream,
            domain_tx_pool_sinks,
            domain_bundle_announcement_sinks,
        }
    }
}

/// Gossip worker to gossip incoming and outgoing messages to other peers.
/// Also, streams the decoded extrinsics to destination domain tx pool if available.
pub struct GossipWorker<Block: BlockT> {
    gossip_engine: Arc<Mutex<GossipEngine<Block>>>,
    gossip_validator: Arc<GossipValidator>,
    gossip_msg_stream: TracingUnboundedReceiver<Message>,
    domain_tx_pool_sinks: BTreeMap<DomainId, DomainTxPoolSink>,
    domain_bundle_announcement_sinks: BTreeMap<DomainId, DomainBundleAnnouncementSink>,
}

/// Returns the network configuration for cross domain message gossip.
pub fn cdm_gossip_peers_set_config() -> NonDefaultSetConfig {
    let mut cfg = NonDefaultSetConfig::new(PROTOCOL_NAME.into(), 5 * 1024 * 1024);
    cfg.allow_non_reserved(25, 25);
    cfg
}

/// Cross domain message topic.
fn topic<Block: BlockT>() -> Block::Hash {
    <<Block::Header as HeaderT>::Hashing as HashT>::hash(b"cross-domain-messages")
}

impl<Block: BlockT> GossipWorker<Block> {
    /// Starts the Gossip message worker.
    pub async fn run(mut self) {
        let mut incoming_cross_domain_messages = Box::pin(
            self.gossip_engine
                .lock()
                .messages_for(topic::<Block>())
                .filter_map(|notification| async move {
                    Message::decode(&mut &notification.message[..])
                        .ok()
                        .map(|msg| (notification.sender, msg))
                }),
        );

        loop {
            let engine = self.gossip_engine.clone();
            let gossip_engine = futures::future::poll_fn(|cx| engine.lock().poll_unpin(cx));

            futures::select! {
                cross_domain_message = incoming_cross_domain_messages.next().fuse() => {
                    if let Some((sender, msg)) = cross_domain_message {
                        tracing::debug!(target: LOG_TARGET, "Incoming cross domain message for domain: {:?}", msg.domain_id);
                        match msg.payload {
                            MessagePayload::EncodedData(data) => self.handle_cross_domain_message(msg.domain_id, data),
                            MessagePayload::BundleAnnouncement(hash) => self.handle_bundle_announcement(
                                msg.domain_id, hash, MessageSource::Network(sender)
                            ),
                        }
                    }
                },

                cross_domain_message = self.gossip_msg_stream.next().fuse() => {
                    if let Some(msg) = cross_domain_message {
                        tracing::debug!(target: LOG_TARGET, "Incoming cross domain message for domain: {:?}", msg.domain_id);
                        match msg.payload {
                            MessagePayload::EncodedData(data) => self.handle_cross_domain_message(msg.domain_id, data),
                            MessagePayload::BundleAnnouncement(hash) => self.handle_bundle_announcement(
                                msg.domain_id, hash, MessageSource::Local
                            ),
                        }
                    }
                }

                _ = gossip_engine.fuse() => {
                    tracing::error!(target: LOG_TARGET, "Gossip engine has terminated.");
                    return;
                }
            }
        }
    }

    fn handle_cross_domain_message(&mut self, domain_id: DomainId, encoded_data: Vec<u8>) {
        // mark and rebroadcast message
        let msg = Message {
            domain_id,
            payload: MessagePayload::EncodedData(encoded_data.clone()),
        };
        let encoded_msg = msg.encode();
        self.gossip_validator.note_broadcast(&encoded_msg);
        self.gossip_engine
            .lock()
            .gossip_message(topic::<Block>(), encoded_msg, false);

        let sink = match self.domain_tx_pool_sinks.get(&domain_id) {
            Some(sink) => sink,
            None => return,
        };

        // send the message to the open and ready channel
        if !sink.is_closed() && sink.unbounded_send(encoded_data).is_ok() {
            return;
        }

        // sink is either closed or failed to send unbounded message
        // consider it closed and remove the sink.
        tracing::error!(
            target: LOG_TARGET,
            "Failed to send incoming domain message: {:?}",
            domain_id
        );
        self.domain_tx_pool_sinks.remove(&domain_id);
    }

    /// Handles the bundle announcements generated locally and received from
    /// the network
    fn handle_bundle_announcement(
        &mut self,
        domain_id: DomainId,
        bundle_hash: SignedBundleHash,
        source: MessageSource,
    ) {
        tracing::info!(
            target: LOG_TARGET,
            "xxx: cdm gossip received: {domain_id:?}, {source:?}, {bundle_hash:?}"
        );
        match source {
            MessageSource::Network(Some(sender)) => {
                // Received announcement from network, forward it to the local announcement
                // listener to initiate download
                let sink = match self.domain_bundle_announcement_sinks.get(&domain_id) {
                    Some(sink) => sink,
                    None => return,
                };

                // send the message to the open and ready channel
                if !sink.is_closed() && sink.unbounded_send((sender, bundle_hash)).is_ok() {
                    return;
                }

                // sink is either closed or failed to send unbounded message
                // consider it closed and remove the sink.
                tracing::error!(
                    target: LOG_TARGET,
                    "Failed to send incoming bundle announcement to listener: {domain_id:?}, {bundle_hash:?}",
                );
                self.domain_bundle_announcement_sinks.remove(&domain_id);
            }
            MessageSource::Local => {
                // Received from local source, broadcast to the network
                let msg = Message {
                    domain_id,
                    payload: bundle_hash.into(),
                };
                let encoded_msg = msg.encode();
                self.gossip_validator.note_broadcast(&encoded_msg);
                self.gossip_engine
                    .lock()
                    .gossip_message(topic::<Block>(), encoded_msg, false);
            }
            _ => {}
        }
    }
}

/// Gossip validator to retain or clean up Gossiped messages.
#[derive(Debug, Default)]
struct GossipValidator {
    should_broadcast: RwLock<HashSet<MessageHash>>,
}

impl GossipValidator {
    fn note_broadcast(&self, msg: &[u8]) {
        let msg_hash = twox_256(msg);
        let mut msg_set = self.should_broadcast.write();
        msg_set.insert(msg_hash);
    }

    fn should_broadcast(&self, msg: &[u8]) -> bool {
        let msg_hash = twox_256(msg);
        let msg_set = self.should_broadcast.read();
        msg_set.contains(&msg_hash)
    }

    fn note_broadcasted(&self, msg: &[u8]) {
        let msg_hash = twox_256(msg);
        let mut msg_set = self.should_broadcast.write();
        msg_set.remove(&msg_hash);
    }
}

impl<Block: BlockT> Validator<Block> for GossipValidator {
    fn validate(
        &self,
        _context: &mut dyn ValidatorContext<Block>,
        _sender: &PeerId,
        mut data: &[u8],
    ) -> ValidationResult<Block::Hash> {
        match Message::decode(&mut data) {
            Ok(_) => ValidationResult::ProcessAndKeep(topic::<Block>()),
            Err(_) => ValidationResult::Discard,
        }
    }

    fn message_expired<'a>(&'a self) -> Box<dyn FnMut(Block::Hash, &[u8]) -> bool + 'a> {
        Box::new(move |_topic, data| !self.should_broadcast(data))
    }

    fn message_allowed<'a>(
        &'a self,
    ) -> Box<dyn FnMut(&PeerId, MessageIntent, &Block::Hash, &[u8]) -> bool + 'a> {
        Box::new(move |_who, _intent, _topic, data| {
            let should_broadcast = self.should_broadcast(data);
            if should_broadcast {
                self.note_broadcasted(data)
            }

            should_broadcast
        })
    }
}
