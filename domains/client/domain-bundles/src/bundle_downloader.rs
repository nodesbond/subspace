//! Bundle download related defines.

use sc_network::PeerId;
use sp_domains::{SignedBundle, SignedBundleHash};

/// The serving side of the bundle server. It runs a single instance
/// of the server task that processes the incoming download requests.
#[async_trait::async_trait]
pub trait BundleServer: Send {
    /// Starts the server processing.
    async fn run(&mut self);
}

/// The client side stub to download bundles from peers. This is a handle
/// that can be used to initiate concurrent downloads.
#[async_trait::async_trait]
pub trait BundleDownloader<Extrinsic, Number, Hash, DomainHash>: Send + Sync {
    /// Downloads the bundle specified by the hash from the peer
    async fn download_bundle(
        &self,
        who: PeerId,
        hash: SignedBundleHash,
    ) -> Result<SignedBundle<Extrinsic, Number, Hash, DomainHash>, String>;
}
