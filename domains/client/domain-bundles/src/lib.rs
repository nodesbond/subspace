mod bundle_downloader;
mod bundle_pool;

use sc_transaction_pool_api::{TransactionPool, TxHash};
use sp_domains::{CompactBundle, CompactSignedBundle, SignedBundle};
use sp_runtime::traits::{Block as BlockT, NumberFor};

pub use bundle_downloader::{BundleDownloader, BundleServer};
pub use bundle_pool::{build_bundle_pool, CompactBundlePool, CompactBundlePoolImpl};

pub type CompactBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactBundle<TxHash<Pool>, Number, Hash, DomainHash>;
pub type CompactSignedBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactSignedBundle<TxHash<Pool>, Number, Hash, DomainHash>;

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
