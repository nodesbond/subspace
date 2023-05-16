#![warn(rust_2018_idioms)]

mod gossip_worker;
mod message_listener;

use sc_utils::mpsc::TracingUnboundedSender;

pub use gossip_worker::{
    cdm_gossip_peers_set_config, DomainBundleAnnouncementSink, DomainTxPoolSink, GossipWorker,
    GossipWorkerBuilder, Message,
};
pub use message_listener::start_domain_message_listener;

/// Sink used to submit all the gossip messages.
pub type GossipMessageSink = TracingUnboundedSender<Message>;
