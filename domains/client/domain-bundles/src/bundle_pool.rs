//! Bundle pool related defines.

use crate::{compact_signed_bundle, CompactSignedBundleForPool};
use lru::LruCache;
use parking_lot::Mutex;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::{SignedBundle, SignedBundleHash};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::num::NonZeroUsize;
use std::sync::Arc;

const COMPACT_BUNDLE_CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(512).expect("Not zero; qed");

/// Pool of compact signed bundles.
#[allow(clippy::type_complexity)]
pub trait CompactBundlePool<Block, PBlock, Pool>: Send + Sync
where
    Block: BlockT,
    PBlock: BlockT,
    Pool: TransactionPool<Block = Block>,
{
    /// Converts the signed bundle to the compact version and adds to the pool.
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
#[allow(clippy::type_complexity)]
pub struct CompactBundlePoolImpl<Block: BlockT, PBlock: BlockT, Pool: TransactionPool> {
    compact_bundle_cache: Arc<
        Mutex<
            LruCache<
                SignedBundleHash,
                CompactSignedBundleForPool<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash>,
            >,
        >,
    >,
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
            compact_bundle_cache: Arc::new(Mutex::new(LruCache::new(COMPACT_BUNDLE_CACHE_SIZE))),
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
        let hash = signed_bundle.hash();
        let compact_bundle = compact_signed_bundle::<Block, PBlock, Pool>(
            self.transaction_pool.as_ref(),
            signed_bundle,
        );
        self.compact_bundle_cache.lock().put(hash, compact_bundle);
    }

    fn get(
        &self,
        hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, NumberFor<PBlock>, PBlock::Hash, Block::Hash>>
    {
        self.compact_bundle_cache.lock().get(hash).cloned()
    }

    fn contains(&self, hash: &SignedBundleHash) -> bool {
        self.compact_bundle_cache.lock().get(hash).is_some()
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
