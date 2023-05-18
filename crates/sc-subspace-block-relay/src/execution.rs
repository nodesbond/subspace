//! Relay implementation for domain bundles.

use crate::utils::NetworkWrapper;
use domain_bundles::{BundleDownloader, BundleServer, CompactBundlePool};
use sc_network::types::ProtocolName;
use sc_network::PeerId;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use std::sync::Arc;

struct ExecutionRelayClient<Pool, BundlePool> {
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    transaction_pool: Arc<Pool>,
    bundle_pool: Arc<BundlePool>,
}

#[async_trait::async_trait]
impl<Pool, BundlePool, Extrinsic, Number, Hash, DomainHash>
    BundleDownloader<Extrinsic, Number, Hash, DomainHash> for ExecutionRelayClient<Pool, BundlePool>
where
    Pool: TransactionPool,
    BundlePool: CompactBundlePool<Pool, Number, Hash, DomainHash>,
    Number: Send + Sync,
    Hash: Send + Sync,
    DomainHash: Send + Sync,
{
    async fn download_bundle(
        &self,
        _who: PeerId,
        _hash: &SignedBundleHash,
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

pub fn build_execution_relay<Pool, BundlePool, Extrinsic, Number, Hash, DomainHash>(
    network: Arc<NetworkWrapper>,
    protocol_name: ProtocolName,
    transaction_pool: Arc<Pool>,
    bundle_pool: Arc<BundlePool>,
) -> (
    Arc<dyn BundleDownloader<Extrinsic, Number, Hash, DomainHash>>,
    Arc<dyn BundleServer>,
)
where
    Pool: TransactionPool + 'static,
    BundlePool: CompactBundlePool<Pool, Number, Hash, DomainHash> + 'static,
    Number: Send + Sync,
    Hash: Send + Sync,
    DomainHash: Send + Sync,
{
    let client = Arc::new(ExecutionRelayClient {
        network,
        protocol_name,
        transaction_pool,
        bundle_pool,
    });
    let server = Arc::new(ExecutionRelayServer);
    (client, server)
}
