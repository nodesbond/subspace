//! Bundle pool related defines.

use crate::CompactSignedBundleForPool;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::sync::Arc;

/// Pool of compact signed bundles.
pub trait CompactBundlePool<Block, PBlock, Pool>: Send + Sync
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    /// Converts te signed bundle to the compact version and adds to the pool.
    fn add(
        &self,
        signed_bundle: &SignedBundle<
            Block::Extrinsic,
            NumberFor<PBlock>,
            PBlock::Hash,
            Block::Hash,
        >,
    );

    /// Looks up the signed bundle for the given bundle hash.
    fn get(
        &self,
        hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>;

    /// Checks if the bundle is in the pool.
    fn contains(&self, hash: &SignedBundleHash) -> bool;
}

/// Compact bundle pool implementation.
pub struct CompactBundlePoolImpl<Block, PBlock, Pool> {
    transaction_pool: Arc<Pool>,
    _p: (
        std::marker::PhantomData<Block>,
        std::marker::PhantomData<PBlock>,
        std::marker::PhantomData<Pool>,
    ),
}

impl<Block, PBlock, Pool> CompactBundlePoolImpl<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    pub fn new(transaction_pool: Arc<Pool>) -> Self {
        Self {
            transaction_pool,
            _p: Default::default(),
        }
    }
}

impl<Block, PBlock, Pool> CompactBundlePool<Block, PBlock, Pool>
    for CompactBundlePoolImpl<Block, PBlock, Pool>
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    fn add(
        &self,
        signed_bundle: &SignedBundle<
            Block::Extrinsic,
            NumberFor<PBlock>,
            PBlock::Hash,
            Block::Hash,
        >,
    ) {
        todo!()
    }

    fn get(
        &self,
        hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>
    {
        todo!()
    }

    fn contains(&self, hash: &SignedBundleHash) -> bool {
        todo!()
    }
}

pub fn build_bundle_pool<Block, PBlock, Pool>(
    transaction_pool: Arc<Pool>,
) -> std::sync::Arc<dyn CompactBundlePool<Block, PBlock, Pool>>
where
    Block: BlockT + 'static,
    PBlock: BlockT + 'static,
    Pool: TransactionPool<Block = Block> + 'static,
{
    std::sync::Arc::new(CompactBundlePoolImpl::new(transaction_pool))
}
