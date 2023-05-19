//! Bundle pool related defines.

use crate::CompactSignedBundleForPool;
use sc_transaction_pool_api::TransactionPool;
use sp_domains::SignedBundleHash;

/// Pool of compact signed bundles.
pub trait CompactBundlePool<Pool, Number, Hash, DomainHash>: Send + Sync
where
    Pool: TransactionPool,
    Number: Send + Sync,
    Hash: Send + Sync,
    DomainHash: Send + Sync,
{
    /// Adds an entry to the pool.
    /// The key is the bundle hash, value is the compact signed bundle
    fn add(
        &self,
        hash: SignedBundleHash,
        bundle: CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>,
    );

    /// Looks up the signed bundle for the given bundle hash.
    fn get(
        &self,
        hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>>;

    /// Checks if the bundle is in the pool.
    fn contains(&self, hash: &SignedBundleHash) -> bool;
}

/// Compact bundle pool implementation.
pub struct CompactBundlePoolImpl<Pool, Number, Hash, DomainHash> {
    _p: (
        std::marker::PhantomData<Pool>,
        std::marker::PhantomData<Number>,
        std::marker::PhantomData<Hash>,
        std::marker::PhantomData<DomainHash>,
    ),
}

impl<Pool, Number, Hash, DomainHash> CompactBundlePoolImpl<Pool, Number, Hash, DomainHash> {
    pub fn new() -> Self {
        Self {
            _p: Default::default(),
        }
    }
}

impl<Pool, Number, Hash, DomainHash> CompactBundlePool<Pool, Number, Hash, DomainHash>
    for CompactBundlePoolImpl<Pool, Number, Hash, DomainHash>
where
    Pool: TransactionPool,
    Number: Send + Sync,
    Hash: Send + Sync,
    DomainHash: Send + Sync,
{
    fn add(
        &self,
        _hash: SignedBundleHash,
        _bundle: CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>,
    ) {
        todo!()
    }

    fn get(
        &self,
        _hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>> {
        todo!()
    }

    fn contains(&self, _hash: &SignedBundleHash) -> bool {
        todo!()
    }
}

pub fn build_bundle_pool<Pool, Number, Hash, DomainHash>(
) -> std::sync::Arc<dyn CompactBundlePool<Pool, Number, Hash, DomainHash>>
where
    Pool: TransactionPool + 'static,
    Number: Send + Sync + 'static,
    Hash: Send + Sync + 'static,
    DomainHash: Send + Sync + 'static,
{
    std::sync::Arc::new(CompactBundlePoolImpl::new())
}
