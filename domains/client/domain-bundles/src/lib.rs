#![feature(const_option)]

use sc_transaction_pool_api::{TransactionPool, TxHash};
use sp_domains::{Bundle, CompactBundle, CompactSignedBundle, SignedBundle};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::fmt::Debug;

pub use bundle_downloader::{BundleDownloader, BundleServer};
pub use bundle_pool::{build_bundle_pool, CompactBundlePool, CompactBundlePoolImpl};
pub use bundle_sync::BundleSync;

mod bundle_downloader;
mod bundle_pool;
mod bundle_sync;

pub type CompactBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactBundle<TxHash<Pool>, Number, Hash, DomainHash>;
pub type CompactSignedBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactSignedBundle<TxHash<Pool>, Number, Hash, DomainHash>;

/// What to do with the successfully verified gossip message.
#[derive(Debug)]
pub enum Action {
    /// The message does not have to be re-gossiped.
    Empty,
    /// Gossip the bundle message to other peers.
    RebroadcastBundle,
    /// Gossip the execution exceipt message to other peers.
    RebroadcastExecutionReceipt,
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

/// Builds the compact signed bundle from the signed bundle
pub fn compact_signed_bundle<
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
>(
    transaction_pool: &Pool,
    signed_bundle: &SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
) -> CompactSignedBundle<TxHash<Pool>, NumberFor<PBlock>, PBlock::Hash, Block::Hash> {
    CompactSignedBundle {
        compact_bundle: CompactBundle {
            header: signed_bundle.bundle.header.clone(),
            receipts: signed_bundle.bundle.receipts.clone(),
            extrinsics_hash: signed_bundle
                .bundle
                .extrinsics
                .iter()
                .map(|extrinsic| transaction_pool.hash_of(extrinsic))
                .collect(),
        },
        bundle_solution: signed_bundle.bundle_solution.clone(),
        signature: signed_bundle.signature.clone(),
    }
}

/// Builds the signed bundle from the compact bundle + extrinsics
pub fn signed_bundle<Block: BlockT, PBlock: BlockT, Pool: TransactionPool<Block = Block>>(
    compact_signed_bundle: &CompactSignedBundleForPool<
        Pool,
        NumberFor<PBlock>,
        PBlock::Hash,
        Block::Hash,
    >,
    extrinsics: Vec<Block::Extrinsic>,
) -> SignedBundle<Block::Extrinsic, NumberFor<PBlock>, PBlock::Hash, Block::Hash> {
    SignedBundle {
        bundle: Bundle {
            header: compact_signed_bundle.compact_bundle.header.clone(),
            receipts: compact_signed_bundle.compact_bundle.receipts.clone(),
            extrinsics,
        },
        bundle_solution: compact_signed_bundle.bundle_solution.clone(),
        signature: compact_signed_bundle.signature.clone(),
    }
}
