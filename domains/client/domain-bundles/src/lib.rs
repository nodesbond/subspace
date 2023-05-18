mod bundle_downloader;
pub mod bundle_pool;

use sc_transaction_pool_api::TxHash;
use sp_domains::{CompactBundle, CompactSignedBundle};

pub use bundle_downloader::{BundleDownloader, BundleServer};
pub use bundle_pool::{CompactBundlePool, CompactBundlePoolImpl};

pub type CompactBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactBundle<TxHash<Pool>, Number, Hash, DomainHash>;
pub type CompactSignedBundleForPool<Pool, Number, Hash, DomainHash> =
    CompactSignedBundle<TxHash<Pool>, Number, Hash, DomainHash>;
