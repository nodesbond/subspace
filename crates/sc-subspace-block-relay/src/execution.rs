//! Relay implementation for domain bundles.

use crate::utils::NetworkWrapper;
use domain_bundles::{BundleDownloader, BundleServer, CompactBundlePool};
use sc_network::types::ProtocolName;
use sc_network::PeerId;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::sync::Arc;

struct ExecutionRelayClient<Block, PBlock, Pool> {
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    transaction_pool: Arc<Pool>,
    bundle_pool: Arc<dyn CompactBundlePool<Block, PBlock, Pool>>,
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
        _who: PeerId,
        _hash: SignedBundleHash,
    ) -> Result<SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>, String>
    {
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

pub fn build_execution_relay<Block, PBlock, Pool>(
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
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
        transaction_pool,
        bundle_pool,
    });
    let server = Box::new(ExecutionRelayServer);
    (client, server)
}
