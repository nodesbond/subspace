//! Relay implementation for domain bundles.

use domain_bundles::{BundleDownloader, BundleServer};
use sc_network::PeerId;
use sp_core::H256;
use sp_domains::SignedBundle;

struct ExecutionRelayClient;
/*
struct ExecutionRelayClient<Block, Pool, ProtoClient>
    where
        Block: BlockT,
        Pool: TransactionPool,
        ProtoClient: ProtocolClient<BlockHash<Block>, TxHash<Pool>, Extrinsic<Block>>,
{
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    protocol_client: Arc<ProtoClient>,
    _phantom_data: std::marker::PhantomData<(Block, Pool)>,
}

 */

#[async_trait::async_trait]
impl<Extrinsic, Number, Hash, DomainHash> BundleDownloader<Extrinsic, Number, Hash, DomainHash>
    for ExecutionRelayClient
{
    async fn download_bundle(
        &self,
        _who: PeerId,
        _hash: &H256,
    ) -> Result<SignedBundle<Extrinsic, Number, Hash, DomainHash>, String> {
        todo!()
    }
}

struct ExecutionRelayServer;

#[async_trait::async_trait]
impl BundleServer for ExecutionRelayServer {
    async fn run(&mut self) {
        todo!()
    }
}