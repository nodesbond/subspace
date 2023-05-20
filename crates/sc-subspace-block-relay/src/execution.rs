//! Relay implementation for domain bundles.

use crate::utils::{NetworkPeerHandle, NetworkWrapper};
use crate::{DownloadResult, RelayError, Resolved, LOG_TARGET};
use codec::{Decode, Encode};
use domain_bundles::{
    signed_bundle, BundleDownloader, BundleServer, CompactBundlePool, CompactSignedBundleForPool,
};
use futures::channel::{mpsc, oneshot};
use futures::stream::StreamExt;
use sc_network::request_responses::{IncomingRequest, OutgoingResponse};
use sc_network::types::ProtocolName;
use sc_network::PeerId;
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool, TxHash};
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, trace, warn};

/// Initial request to the server
#[derive(Encode, Decode)]
struct InitialRequest {
    /// Signed bundle hash
    hash: SignedBundleHash,
}

/// Initial response from server
#[derive(Encode, Decode)]
struct InitialResponse<Pool: TransactionPool, Number, Hash, DomainHash> {
    /// The compact signed bundle for the requested hash
    compact_signed_bundle: CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>,
}

/// Request for missing transactions
#[derive(Encode, Decode)]
pub(crate) struct MissingEntriesRequest<ExtrinisicHash> {
    /// Map of missing entry Id -> extrinsic hash
    extrinsic_hash: BTreeMap<u64, ExtrinisicHash>,
}

/// Response for missing transactions
#[derive(Encode, Decode)]
pub(crate) struct MissingEntriesResponse<Extrinisic> {
    /// Map of missing entry Id -> extrinsic
    extrinsics: BTreeMap<u64, Extrinisic>,
}

/// The message to the server
#[derive(Encode, Decode)]
enum ServerMessage<ExtrinisicHash> {
    InitialRequest(InitialRequest),
    MissingEntriesRequest(MissingEntriesRequest<ExtrinisicHash>),
}

struct ResolveContext<ExtrinsicHash, Extrinsic> {
    resolved: BTreeMap<u64, Resolved<ExtrinsicHash, Extrinsic>>,
    local_miss: BTreeMap<u64, ExtrinsicHash>,
}

struct ExecutionRelayClient<Block, PBlock, Pool> {
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    transaction_pool: Arc<Pool>,
    _p: std::marker::PhantomData<(Block, PBlock)>,
}

impl<Block, PBlock, Pool> ExecutionRelayClient<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    async fn download(
        &self,
        who: PeerId,
        hash: SignedBundleHash,
    ) -> Result<
        DownloadResult<
            SignedBundleHash,
            SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
        >,
        RelayError,
    > {
        let start_ts = Instant::now();
        let network_peer_handle = self
            .network
            .network_peer_handle(self.protocol_name.clone(), who)?;

        // Perform the initial request/response
        let initial_request = InitialRequest { hash };
        let initial_response = network_peer_handle
            .request::<_, InitialResponse<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>(
                ServerMessage::<TxHash<Pool>>::InitialRequest(initial_request),
            )
            .await?;

        // Resolve the response to get the bundles
        let compact_signed_bundle = initial_response.compact_signed_bundle;
        let (extrinsics, local_miss) = self
            .resolve_bundles(
                &hash,
                &compact_signed_bundle.compact_bundle.extrinsics_hash,
                &network_peer_handle,
            )
            .await?;

        let bundle = signed_bundle::<Block, PBlock, Pool>(&compact_signed_bundle, extrinsics);
        Ok(DownloadResult {
            download_unit_id: hash,
            downloaded: bundle,
            latency: start_ts.elapsed(),
            local_miss,
        })
    }

    /// Resolves the extrinsic hashes in the compact response to get the
    /// extrinsics
    async fn resolve_bundles(
        &self,
        bundle_hash: &SignedBundleHash,
        extrinsics_hash: &Vec<TxHash<Pool>>,
        network_peer_handle: &NetworkPeerHandle,
    ) -> Result<(Vec<Block::Extrinsic>, usize), RelayError> {
        // Try to resolve the bundles locally first
        let context = self.resolve_local(extrinsics_hash)?;
        if context.resolved.len() == extrinsics_hash.len() {
            trace!(
                target: LOG_TARGET,
                "relay::bundle::resolve: {:?}: resolved locally[{}]",
                bundle_hash,
                extrinsics_hash.len()
            );
            return Ok((
                context
                    .resolved
                    .into_values()
                    .map(|resolved| resolved.protocol_unit)
                    .collect(),
                0,
            ));
        }

        // Resolve the misses from the server
        let misses = context.local_miss.len();
        let resolved = self.resolve_misses(context, network_peer_handle).await?;

        trace!(
            target: LOG_TARGET,
            "relay::bundle::resolve: {:?}: resolved by server[{},{}]",
            bundle_hash,
            resolved.len(),
            misses,
        );
        Ok((resolved, misses))
    }

    /// Tries to resolve the entries in InitialResponse locally
    fn resolve_local(
        &self,
        extrinsics_hash: &[TxHash<Pool>],
    ) -> Result<ResolveContext<TxHash<Pool>, Block::Extrinsic>, RelayError> {
        let mut context: ResolveContext<TxHash<Pool>, Block::Extrinsic> = ResolveContext {
            resolved: BTreeMap::new(),
            local_miss: BTreeMap::new(),
        };

        for (index, extrinsic_hash) in extrinsics_hash.iter().enumerate() {
            // Look up the extrinsic hash in the transaction pool
            match self.transaction_pool.ready_transaction(extrinsic_hash) {
                Some(in_pool) => {
                    context.resolved.insert(
                        index as u64,
                        Resolved {
                            protocol_unit_id: extrinsic_hash.clone(),
                            protocol_unit: in_pool.data().clone(),
                            locally_resolved: true,
                        },
                    );
                }
                None => {
                    context
                        .local_miss
                        .insert(index as u64, extrinsic_hash.clone());
                }
            }
        }
        Ok(context)
    }

    /// Fetches the missing entries from the server
    async fn resolve_misses(
        &self,
        context: ResolveContext<TxHash<Pool>, Block::Extrinsic>,
        network_peer_handle: &NetworkPeerHandle,
    ) -> Result<Vec<Block::Extrinsic>, RelayError> {
        let ResolveContext {
            mut resolved,
            local_miss,
        } = context;
        let missing = local_miss.len();

        // Request the missing entries from the server
        let request = ServerMessage::MissingEntriesRequest(MissingEntriesRequest {
            extrinsic_hash: local_miss.clone(),
        });
        let response: MissingEntriesResponse<Block::Extrinsic> =
            network_peer_handle.request(request).await?;

        if response.extrinsics.len() != missing {
            return Err(RelayError::ResolveMismatch {
                expected: missing,
                actual: response.extrinsics.len(),
            });
        }

        // Merge the resolved entries from the server
        for (missing_key, extrinsic_hash) in local_miss.into_iter() {
            if let Some(extrinsic) = response.extrinsics.get(&missing_key) {
                resolved.insert(
                    missing_key,
                    Resolved {
                        protocol_unit_id: extrinsic_hash,
                        protocol_unit: extrinsic.clone(),
                        locally_resolved: false,
                    },
                );
            } else {
                return Err(RelayError::ResolvedNotFound(missing));
            }
        }
        Ok(resolved
            .into_values()
            .map(|resolved| resolved.protocol_unit)
            .collect())
    }
}

#[async_trait::async_trait]
impl<Block, PBlock, Pool> BundleDownloader<Block, PBlock, Pool>
    for ExecutionRelayClient<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    async fn download_bundle(
        &self,
        who: PeerId,
        hash: SignedBundleHash,
    ) -> Result<SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>, String>
    {
        let ret = self.download(who, hash).await;
        match ret {
            Ok(result) => {
                info!(
                    target: LOG_TARGET,
                    "relay::download_bundle: {:?} => {},{},{:?}",
                    result.download_unit_id,
                    result.downloaded.size_hint(),
                    result.local_miss,
                    result.latency
                );
                Ok(result.downloaded)
            }
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "relay::download_bundle: peer = {who:?}, err = {err:?}"
                );
                Err(format!("{err:?}"))
            }
        }
    }
}

struct ExecutionRelayServer<Block, PBlock, Pool> {
    transaction_pool: Arc<Pool>,
    bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
    request_receiver: mpsc::Receiver<IncomingRequest>,
}

impl<Block, PBlock, Pool> ExecutionRelayServer<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    async fn on_request(&mut self, request: IncomingRequest) {
        let IncomingRequest {
            peer,
            payload,
            pending_response,
        } = request;
        let server_msg: ServerMessage<TxHash<Pool>> = match Decode::decode(&mut payload.as_ref()) {
            Ok(msg) => msg,
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "relay::bundle::on_request: decode incoming: {peer}: {err:?}"
                );
                return;
            }
        };

        let ret = match server_msg {
            ServerMessage::InitialRequest(req) => self.on_initial_request(req),
            ServerMessage::MissingEntriesRequest(req) => self.on_missing_entries_request(req),
        };

        match ret {
            Ok(response) => {
                self.send_response(peer, response, pending_response);
                trace!(
                    target: LOG_TARGET,
                    "relay::bundle server: request processed from: {peer}"
                );
            }
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "relay::bundle server: request processing error: {peer}:  {err:?}"
                );
            }
        }
    }

    /// Handles the initial request from the client
    fn on_initial_request(&mut self, request: InitialRequest) -> Result<Vec<u8>, RelayError> {
        // Look up the bundle hash in the compact bunlde pool
        if let Some(compact_signed_bundle) = self.bundle_pool.get(&request.hash) {
            let response: InitialResponse<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash> =
                InitialResponse {
                    compact_signed_bundle,
                };
            Ok(response.encode())
        } else {
            Err(RelayError::CompactBlockNotFound(request.hash))
        }
    }

    /// Handles the missing entries request
    fn on_missing_entries_request(
        &self,
        request: MissingEntriesRequest<TxHash<Pool>>,
    ) -> Result<Vec<u8>, RelayError> {
        let mut extrinsics = BTreeMap::new();
        for (id, extrinsic_hash) in request.extrinsic_hash {
            // Look up the extrinsic hash in the transaction pool
            if let Some(in_pool) = self.transaction_pool.ready_transaction(&extrinsic_hash) {
                extrinsics.insert(id, in_pool.data().clone());
            } else {
                warn!(
                    target: LOG_TARGET,
                    "relay::bundle::on_missing_entries_request: missing entry not found: {extrinsic_hash:?}"
                );
            }
        }
        Ok(MissingEntriesResponse { extrinsics }.encode())
    }

    /// Builds/sends the response back to the client
    fn send_response(
        &self,
        peer: PeerId,
        response: Vec<u8>,
        sender: oneshot::Sender<OutgoingResponse>,
    ) {
        let response = OutgoingResponse {
            result: Ok(response),
            reputation_changes: Vec::new(),
            sent_feedback: None,
        };
        if sender.send(response).is_err() {
            warn!(
                target: LOG_TARGET,
                "relay::bundle::send_response: failed to send to {peer}"
            );
        }
    }
}

#[async_trait::async_trait]
impl<Block, PBlock, Pool> BundleServer for ExecutionRelayServer<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    async fn run(&mut self) {
        info!(target: LOG_TARGET, "relay::domain bundle server: starting");
        while let Some(request) = self.request_receiver.next().await {
            self.on_request(request).await;
        }
    }
}

pub fn build_execution_relay<Block, PBlock, Pool>(
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    request_receiver: mpsc::Receiver<IncomingRequest>,
    transaction_pool: Arc<Pool>,
    bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
) -> (
    Arc<dyn BundleDownloader<Block, PBlock, Pool>>,
    Box<dyn BundleServer>,
)
where
    Block: BlockT + 'static,
    PBlock: BlockT + 'static,
    Pool: TransactionPool<Block = Block> + 'static,
{
    let client = Arc::new(ExecutionRelayClient {
        network,
        protocol_name,
        transaction_pool: transaction_pool.clone(),
        _p: Default::default(),
    });
    let server = Box::new(ExecutionRelayServer {
        transaction_pool,
        bundle_pool,
        request_receiver,
    });
    (client, server)
}
