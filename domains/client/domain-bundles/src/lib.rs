mod bundle_downloader;
mod bundle_pool;

use futures::channel::mpsc::{self, Receiver};
use sc_service::config::{IncomingRequest, RequestResponseConfig};
use sc_transaction_pool_api::TxHash;
use sp_domains::{CompactBundle, CompactSignedBundle};
use std::time::Duration;

pub use bundle_downloader::{BundleDownloader, BundleServer};
pub use bundle_pool::{CompactBundlePool, CompactBundlePoolImpl};

pub type CompactBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactBundle<TxHash<Pool>, Number, Hash, DomainHash>;
pub type CompactSignedBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactSignedBundle<TxHash<Pool>, Number, Hash, DomainHash>;

/// Bundle relay config
#[derive(Debug)]
pub struct BundleRelayConfig {
    /// Request/response protocol config
    pub request_response_protocol: RequestResponseConfig,

    /// Receiver for the incoming requests
    request_receiver: Receiver<IncomingRequest>,
}

impl BundleRelayConfig {
    pub fn new(protocol_name: String, incoming_queue_size: usize) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(incoming_queue_size);
        Self {
            request_response_protocol: RequestResponseConfig {
                name: protocol_name.into(),
                fallback_names: Vec::new(),
                max_request_size: 1024 * 1024,
                max_response_size: 16 * 1024 * 1024,
                request_timeout: Duration::from_secs(20),
                inbound_queue: Some(request_sender),
            },
            request_receiver,
        }
    }
}
