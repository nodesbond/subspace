//! Compact bundle pool related defines.

use sp_core::H256;
use sp_domains::CompactSignedBundle;

/// Pool of compact signed bundles.
pub trait CompactBundlePool<ExtrinsicHash, Number, Hash, DomainHash> {
    /// Adds an entry to the pool.
    fn add(bundle: CompactSignedBundle<ExtrinsicHash, Number, Hash, DomainHash>);

    /// Looks up the signed bundle for the given hash.
    fn get(hash: &H256) -> Option<CompactSignedBundle<ExtrinsicHash, Number, Hash, DomainHash>>;
}

/// Compact bundle pool implementation.
pub struct CompactBundlePoolImpl<ExtrinsicHash, Number, Hash, DomainHash> {
    _p: (
        std::marker::PhantomData<ExtrinsicHash>,
        std::marker::PhantomData<Number>,
        std::marker::PhantomData<Hash>,
        std::marker::PhantomData<DomainHash>,
    ),
}

impl<ExtrinsicHash, Number, Hash, DomainHash>
    CompactBundlePool<ExtrinsicHash, Number, Hash, DomainHash>
    for CompactBundlePoolImpl<ExtrinsicHash, Number, Hash, DomainHash>
{
    fn add(_bundle: CompactSignedBundle<ExtrinsicHash, Number, Hash, DomainHash>) {
        todo!()
    }

    fn get(_hash: &H256) -> Option<CompactSignedBundle<ExtrinsicHash, Number, Hash, DomainHash>> {
        todo!()
    }
}
