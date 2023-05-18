//! Bundle pool related defines.

use crate::CompactSignedBundleForPool;
use sc_transaction_pool_api::TransactionPool;
use sp_core::H256;
use sp_domains::{CompactSignedBundle, SignedBundleHash};

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
        hash: SignedBundleHash,
        bundle: CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>,
    ) {
        todo!()
    }

    /// Looks up the signed bundle for the given bundle hash.
    fn get(
        &self,
        hash: &SignedBundleHash,
    ) -> Option<CompactSignedBundleForPool<Pool, Number, Hash, DomainHash>> {
        todo!()
    }
}
