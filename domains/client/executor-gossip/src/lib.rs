mod sync;
mod worker;

use self::worker::GossipWorker;
use domain_bundles::{BundleDownloader, CompactBundlePool};
use parity_scale_codec::{Decode, Encode};
use parking_lot::{Mutex, RwLock};
use sc_network::config::NonDefaultSetConfig;
use sc_network::PeerId;
use sc_network_common::role::ObservedRole;
use sc_network_gossip::{
    GossipEngine, MessageIntent, Network as GossipNetwork, Syncing as GossipSyncing,
    ValidationResult, Validator, ValidatorContext,
};
use sc_transaction_pool_api::TransactionPool;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_core::hashing::twox_64;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, Hash as HashT, Header as HeaderT, NumberFor};
use std::collections::HashSet;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};

const LOG_TARGET: &str = "gossip::executor";

const EXECUTOR_PROTOCOL_NAME: &str = "/subspace/executor/1";

// TODO: proper timeout
/// Timeout for rebroadcasting messages.
/// The default value used in network-gossip is 1100ms.
const REBROADCAST_AFTER: Duration = Duration::from_secs(6);

type MessageHash = [u8; 8];

/// Returns the configuration value to put in [`sc_network::config::NetworkConfiguration::extra_sets`].
pub fn executor_gossip_peers_set_config() -> NonDefaultSetConfig {
    let mut cfg = NonDefaultSetConfig::new(EXECUTOR_PROTOCOL_NAME.into(), 1024 * 1024);
    cfg.allow_non_reserved(25, 25);
    cfg
}

/// Gossip engine messages topic.
fn topic<Block: BlockT>() -> Block::Hash {
    <<Block::Header as HeaderT>::Hashing as HashT>::hash(b"executor")
}

/// Executor gossip message type.
///
/// This is the root type that gets encoded and sent on the network.
#[derive(Debug, Encode, Decode)]
pub enum GossipMessage<PBlock: BlockT, Block: BlockT> {
    // Retaining Bundle() for backward compatibility
    Bundle(SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>),
    BundleHash(SignedBundleHash),
}

impl<PBlock: BlockT, Block: BlockT>
    From<SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>
    for GossipMessage<PBlock, Block>
{
    #[inline]
    fn from(
        bundle: SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
    ) -> Self {
        Self::BundleHash(bundle.hash())
    }
}

/// What to do with the successfully verified gossip message.
#[derive(Debug)]
pub enum Action {
    /// The message does not have to be re-gossiped.
    Empty,
    /// Gossip the bundle message to other peers.
    RebroadcastBundle,
}

impl Action {
    pub fn rebroadcast_bundle(&self) -> bool {
        matches!(self, Self::RebroadcastBundle)
    }
}

/// Handler for the messages received from the executor gossip network.
pub trait GossipMessageHandler<PBlock, Block>
where
    PBlock: BlockT,
    Block: BlockT,
{
    /// Error type.
    type Error: Debug;

    /// Validates and applies when a transaction bundle was received.
    fn on_bundle(
        &self,
        bundle: &SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
    ) -> Result<Action, Self::Error>;
}

/// Validator for the gossip messages.
pub struct GossipValidator<PBlock, Block, Executor>
where
    PBlock: BlockT,
    Block: BlockT,
    Executor: GossipMessageHandler<PBlock, Block>,
{
    topic: Block::Hash,
    executor: Executor,
    next_rebroadcast: Mutex<Instant>,
    known_rebroadcasted: RwLock<HashSet<MessageHash>>,
    _phantom_data: PhantomData<PBlock>,
}

impl<PBlock, Block, Executor> GossipValidator<PBlock, Block, Executor>
where
    PBlock: BlockT,
    Block: BlockT,
    Executor: GossipMessageHandler<PBlock, Block>,
{
    pub fn new(executor: Executor) -> Self {
        Self {
            topic: topic::<Block>(),
            executor,
            next_rebroadcast: Mutex::new(Instant::now() + REBROADCAST_AFTER),
            known_rebroadcasted: RwLock::new(HashSet::new()),
            _phantom_data: PhantomData::default(),
        }
    }

    pub(crate) fn note_rebroadcasted(&self, encoded_message: &[u8]) {
        let mut known_rebroadcasted = self.known_rebroadcasted.write();
        known_rebroadcasted.insert(twox_64(encoded_message));
    }

    fn validate_message(&self, msg: GossipMessage<PBlock, Block>) -> ValidationResult<Block::Hash> {
        match msg {
            GossipMessage::Bundle(_) => ValidationResult::Discard,
            GossipMessage::BundleHash(_) => ValidationResult::ProcessAndDiscard(self.topic),
        }
    }

    pub(crate) fn validate_bundle(
        &self,
        bundle: &SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
    ) -> Action {
        let outcome = self.executor.on_bundle(&bundle);
        match outcome {
            Ok(action) if action.rebroadcast_bundle() => Action::RebroadcastBundle,
            Err(err) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?err,
                    "Invalid GossipMessage::Bundle discarded"
                );
                Action::Empty
            }
            _ => Action::Empty,
        }
    }
}

impl<PBlock, Block, Executor> Validator<Block> for GossipValidator<PBlock, Block, Executor>
where
    PBlock: BlockT,
    Block: BlockT,
    Executor: GossipMessageHandler<PBlock, Block> + Send + Sync,
{
    fn new_peer(
        &self,
        _context: &mut dyn ValidatorContext<Block>,
        _who: &PeerId,
        _role: ObservedRole,
    ) {
    }

    fn peer_disconnected(&self, _context: &mut dyn ValidatorContext<Block>, _who: &PeerId) {}

    fn validate(
        &self,
        _context: &mut dyn ValidatorContext<Block>,
        _sender: &PeerId,
        mut data: &[u8],
    ) -> ValidationResult<Block::Hash> {
        match GossipMessage::<PBlock, Block>::decode(&mut data) {
            Ok(msg) => {
                tracing::debug!(target: LOG_TARGET, ?msg, "Validating incoming message");
                self.validate_message(msg)
            }
            Err(err) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?err,
                    ?data,
                    "Message discarded due to the decoding error"
                );
                ValidationResult::Discard
            }
        }
    }

    /// Produce a closure for validating messages on a given topic.
    ///
    /// The gossip engine will periodically prune old or no longer relevant messages using
    /// `message_expired`.
    fn message_expired<'a>(&'a self) -> Box<dyn FnMut(Block::Hash, &[u8]) -> bool + 'a> {
        Box::new(move |_topic, mut data| {
            let msg_hash = twox_64(data);
            // TODO: can be expired due to the message itself might be too old?
            let _msg = match GossipMessage::<PBlock, Block>::decode(&mut data) {
                Ok(msg) => msg,
                Err(_) => return true,
            };
            let expired = {
                let known_rebroadcasted = self.known_rebroadcasted.read();
                known_rebroadcasted.contains(&msg_hash)
            };
            if expired {
                let mut known_rebroadcasted = self.known_rebroadcasted.write();
                known_rebroadcasted.remove(&msg_hash);
            }
            expired
        })
    }

    /// Produce a closure for filtering egress messages.
    ///
    /// Called before actually sending a message to a peer.
    fn message_allowed<'a>(
        &'a self,
    ) -> Box<dyn FnMut(&PeerId, MessageIntent, &Block::Hash, &[u8]) -> bool + 'a> {
        let do_rebroadcast = {
            let now = Instant::now();
            let mut next_rebroadcast = self.next_rebroadcast.lock();
            if now >= *next_rebroadcast {
                *next_rebroadcast = now + REBROADCAST_AFTER;
                true
            } else {
                false
            }
        };

        Box::new(move |_who, intent, _topic, mut data| {
            if let MessageIntent::PeriodicRebroadcast = intent {
                return do_rebroadcast;
            }

            GossipMessage::<PBlock, Block>::decode(&mut data).is_ok()
        })
    }
}

type BundleReceiver = TracingUnboundedReceiver<SignedBundleHash>;

/// Parameters to run the executor gossip service.
pub struct ExecutorGossipParams<
    Network,
    GossipSync,
    Executor,
    Pool,
    Extrinsic,
    Number,
    Hash,
    DomainHash,
> {
    /// Substrate network service.
    pub network: Network,
    /// Syncing service an event stream for peers.
    pub sync: Arc<GossipSync>,
    /// Executor instance.
    pub executor: Executor,
    /// Stream of transaction bundle produced locally.
    pub bundle_receiver: BundleReceiver,
    /// Components for bundle syncing if enabled.
    pub bundle_sync: Option<(
        Arc<dyn CompactBundlePool<Pool, Number, Hash, DomainHash>>,
        Arc<dyn BundleDownloader<Extrinsic, Number, Hash, DomainHash>>,
    )>,
}

/// Starts the executor gossip worker.
pub async fn start_gossip_worker<PBlock, Block, Network, GossipSync, Executor, Pool>(
    gossip_params: ExecutorGossipParams<
        Network,
        GossipSync,
        Executor,
        Pool,
        Block::Extrinsic,
        NumberFor<PBlock>,
        PBlock::Hash,
        Block::Hash,
    >,
) where
    PBlock: BlockT,
    Block: BlockT,
    Network: GossipNetwork<Block> + Send + Sync + Clone + 'static,
    Executor: GossipMessageHandler<PBlock, Block> + Send + Sync + 'static,
    GossipSync: GossipSyncing<Block> + 'static,
    Pool: TransactionPool + 'static,
{
    let ExecutorGossipParams {
        network,
        sync,
        executor,
        bundle_receiver,
        bundle_sync,
    } = gossip_params;

    let gossip_validator = Arc::new(GossipValidator::new(executor));
    let gossip_engine = GossipEngine::new(
        network,
        sync,
        EXECUTOR_PROTOCOL_NAME,
        gossip_validator.clone(),
        None,
    );

    let gossip_worker = GossipWorker::new(
        gossip_validator,
        Arc::new(Mutex::new(gossip_engine)),
        bundle_receiver,
        bundle_sync,
    );

    gossip_worker.run().await
}
