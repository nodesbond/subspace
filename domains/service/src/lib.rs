//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

mod core_domain;
mod core_domain_tx_pre_validator;
pub mod providers;
pub mod rpc;
mod system_domain;
mod system_domain_tx_pre_validator;

pub use self::core_domain::{new_full_core, CoreDomainExecutor, CoreDomainParams, NewFullCore};
pub use self::core_domain_tx_pre_validator::CoreDomainTxPreValidator;
pub use self::system_domain::{new_full_system, FullPool, NewFullSystem};
use futures::channel::mpsc::{self, Receiver};
use sc_executor::NativeElseWasmExecutor;
use sc_service::config::{IncomingRequest, RequestResponseConfig};
use sc_service::{Configuration as ServiceConfiguration, TFullClient};
use std::time::Duration;

/// Domain full client.
pub type FullClient<Block, RuntimeApi, ExecutorDispatch> =
    TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;

pub type FullBackend<Block> = sc_service::TFullBackend<Block>;

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

/// Domain configuration.
#[derive(Debug)]
pub struct DomainConfiguration<AccountId> {
    pub service_config: ServiceConfiguration,
    pub maybe_relayer_id: Option<AccountId>,
    /// The domain bundle relay config if enabled
    pub bundle_relay_config: Option<BundleRelayConfig>,
}
