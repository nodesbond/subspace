// Copyright (C) 2021 Subspace Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Primitives for domains pallet.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod bundle_producer_election;
pub mod fraud_proof;
pub mod merkle_tree;
pub mod transaction;

use bundle_producer_election::{BundleProducerElectionParams, VrfProofError};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_api::RuntimeVersion;
use sp_core::crypto::KeyTypeId;
use sp_core::sr25519::vrf::{VrfOutput, VrfProof, VrfSignature};
use sp_core::H256;
use sp_runtime::generic::OpaqueDigestItemId;
use sp_runtime::traits::{
    BlakeTwo256, Block as BlockT, CheckedAdd, Hash as HashT, NumberFor, Zero,
};
use sp_runtime::{DigestItem, OpaqueExtrinsic, Percent, RuntimeAppPublic};
use sp_runtime_interface::pass_by::PassBy;
use sp_runtime_interface::{pass_by, runtime_interface};
use sp_std::vec::Vec;
use sp_weights::Weight;
use subspace_core_primitives::crypto::blake2b_256_hash;
use subspace_core_primitives::{Blake2b256Hash, Randomness, U256};
use subspace_runtime_primitives::{Balance, Moment};

/// Key type for Operator.
const KEY_TYPE: KeyTypeId = KeyTypeId(*b"oper");

mod app {
    use super::KEY_TYPE;
    use sp_application_crypto::{app_crypto, sr25519};

    app_crypto!(sr25519, KEY_TYPE);
}

/// An operator authority signature.
pub type OperatorSignature = app::Signature;

/// An operator authority keypair. Necessarily equivalent to the schnorrkel public key used in
/// the main executor module. If that ever changes, then this must, too.
#[cfg(feature = "std")]
pub type OperatorPair = app::Pair;

/// An operator authority identifier.
pub type OperatorPublicKey = app::Public;

/// A type that implements `BoundToRuntimeAppPublic`, used for operator signing key.
pub struct OperatorKey;

impl sp_runtime::BoundToRuntimeAppPublic for OperatorKey {
    type Public = OperatorPublicKey;
}

/// Stake weight in the domain bundle election.
///
/// Derived from the Balance and can't be smaller than u128.
pub type StakeWeight = u128;

/// Unique identifier of a domain.
#[derive(
    Clone,
    Copy,
    Debug,
    Hash,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Encode,
    Decode,
    TypeInfo,
    Serialize,
    Deserialize,
    MaxEncodedLen,
)]
pub struct DomainId(u32);

impl From<u32> for DomainId {
    #[inline]
    fn from(x: u32) -> Self {
        Self(x)
    }
}

impl From<DomainId> for u32 {
    #[inline]
    fn from(domain_id: DomainId) -> Self {
        domain_id.0
    }
}

impl core::ops::Add<DomainId> for DomainId {
    type Output = Self;

    fn add(self, other: DomainId) -> Self {
        Self(self.0 + other.0)
    }
}

impl core::ops::Sub<DomainId> for DomainId {
    type Output = Self;

    fn sub(self, other: DomainId) -> Self {
        Self(self.0 - other.0)
    }
}

impl CheckedAdd for DomainId {
    fn checked_add(&self, rhs: &Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }
}

impl DomainId {
    /// Creates a [`DomainId`].
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Converts the inner integer to little-endian bytes.
    pub fn to_le_bytes(&self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
}

/// Unsealed header of bundle.
///
/// Domain operator needs to sign the hash of [`BundleHeader`] and uses the signature to
/// assemble the final [`SealedBundleHeader`].
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct BundleHeader<Number, Hash, DomainHash> {
    /// The block number of consensus block at which the bundle was created.
    pub consensus_block_number: Number,
    /// The hash of consensus block corresponding to `consensus_block_number`.
    pub consensus_block_hash: Hash,
    /// The merkle root of the extrinsics.
    pub extrinsics_root: H256,
    /// Proof of bundle producer election.
    pub proof_of_election: ProofOfElection<DomainHash>,
}

impl<Number: Encode, Hash: Encode, DomainHash: Encode> BundleHeader<Number, Hash, DomainHash> {
    /// Returns the hash of this header.
    pub fn hash(&self) -> H256 {
        BlakeTwo256::hash_of(self)
    }
}

/// Header of bundle.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct SealedBundleHeader<Number, Hash, DomainHash> {
    /// Unsealed header.
    pub header: BundleHeader<Number, Hash, DomainHash>,
    /// Signature of the bundle.
    pub signature: OperatorSignature,
}

impl<Number: Encode, Hash: Encode, DomainHash: Encode>
    SealedBundleHeader<Number, Hash, DomainHash>
{
    /// Constructs a new instance of [`SealedBundleHeader`].
    pub fn new(
        header: BundleHeader<Number, Hash, DomainHash>,
        signature: OperatorSignature,
    ) -> Self {
        Self { header, signature }
    }

    /// Returns the hash of the inner unsealed header.
    pub fn pre_hash(&self) -> H256 {
        self.header.hash()
    }

    /// Returns the hash of this header.
    pub fn hash(&self) -> H256 {
        BlakeTwo256::hash_of(self)
    }

    pub fn slot_number(&self) -> u64 {
        self.header.proof_of_election.slot_number
    }

    /// Returns whether the signature is valid.
    pub fn verify_signature(&self) -> bool {
        self.header
            .proof_of_election
            .operator_public_key
            .verify(&self.pre_hash(), &self.signature)
    }
}

#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct ProofOfElection<DomainHash> {
    /// Domain id.
    pub domain_id: DomainId,
    /// The slot number.
    pub slot_number: u64,
    /// Global challenge.
    pub global_challenge: Blake2b256Hash,
    /// VRF signature.
    pub vrf_signature: VrfSignature,
    // TODO: operator_id
    /// VRF public key.
    pub operator_public_key: OperatorPublicKey,
    // TODO: added temporarily in order to not change a lot of code to make it compile, remove later.
    pub _phandom: DomainHash,
}

impl<DomainHash> ProofOfElection<DomainHash> {
    pub fn verify_vrf_proof(&self) -> Result<(), VrfProofError> {
        bundle_producer_election::verify_vrf_proof(
            self.domain_id,
            &self.operator_public_key,
            &self.vrf_signature,
            &self.global_challenge,
        )
    }

    /// Computes the VRF hash.
    pub fn vrf_hash(&self) -> Blake2b256Hash {
        let mut bytes = self.vrf_signature.output.encode();
        bytes.append(&mut self.vrf_signature.proof.encode());
        blake2b_256_hash(&bytes)
    }
}

impl<DomainHash: Default> ProofOfElection<DomainHash> {
    #[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
    pub fn dummy(domain_id: DomainId, operator_public_key: OperatorPublicKey) -> Self {
        let output_bytes = vec![0u8; VrfOutput::max_encoded_len()];
        let proof_bytes = vec![0u8; VrfProof::max_encoded_len()];
        let vrf_signature = VrfSignature {
            output: VrfOutput::decode(&mut output_bytes.as_slice()).unwrap(),
            proof: VrfProof::decode(&mut proof_bytes.as_slice()).unwrap(),
        };
        Self {
            domain_id,
            slot_number: 0u64,
            global_challenge: Blake2b256Hash::default(),
            vrf_signature,
            operator_public_key,
            _phandom: Default::default(),
        }
    }
}

/// Domain bundle.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct Bundle<Extrinsic, Number, Hash, DomainNumber, DomainHash> {
    /// Sealed bundle header.
    pub sealed_header: SealedBundleHeader<Number, Hash, DomainHash>,
    /// Execution receipt that should extend the receipt chain or add confirmations
    /// to the head receipt.
    pub receipt: ExecutionReceipt<Number, Hash, DomainNumber, DomainHash>,
    /// The accompanying extrinsics.
    pub extrinsics: Vec<Extrinsic>,
}

impl<Extrinsic: Encode, Number: Encode, Hash: Encode, DomainNumber: Encode, DomainHash: Encode>
    Bundle<Extrinsic, Number, Hash, DomainNumber, DomainHash>
{
    /// Returns the hash of this bundle.
    pub fn hash(&self) -> H256 {
        BlakeTwo256::hash_of(self)
    }

    /// Returns the domain_id of this bundle.
    pub fn domain_id(&self) -> DomainId {
        self.sealed_header.header.proof_of_election.domain_id
    }

    /// Consumes [`Bundle`] to extract the inner operator public key.
    pub fn into_operator_public_key(self) -> OperatorPublicKey {
        self.sealed_header
            .header
            .proof_of_election
            .operator_public_key
    }
}

/// Bundle with opaque extrinsics.
pub type OpaqueBundle<Number, Hash, DomainNumber, DomainHash> =
    Bundle<OpaqueExtrinsic, Number, Hash, DomainNumber, DomainHash>;

impl<Extrinsic: Encode, Number, Hash, DomainNumber, DomainHash>
    Bundle<Extrinsic, Number, Hash, DomainNumber, DomainHash>
{
    /// Convert a bundle with generic extrinsic to a bundle with opaque extrinsic.
    pub fn into_opaque_bundle(self) -> OpaqueBundle<Number, Hash, DomainNumber, DomainHash> {
        let Bundle {
            sealed_header,
            receipt,
            extrinsics,
        } = self;
        let opaque_extrinsics = extrinsics
            .into_iter()
            .map(|xt| {
                OpaqueExtrinsic::from_bytes(&xt.encode())
                    .expect("We have just encoded a valid extrinsic; qed")
            })
            .collect();
        OpaqueBundle {
            sealed_header,
            receipt,
            extrinsics: opaque_extrinsics,
        }
    }
}

/// Receipt of a domain block execution.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct ExecutionReceipt<Number, Hash, DomainNumber, DomainHash> {
    /// Consensus block number.
    pub consensus_block_number: Number,
    /// Hash of the origin consensus block this receipt corresponds to.
    pub consensus_block_hash: Hash,
    /// Domain block number.
    pub domain_block_number: DomainNumber,
    /// Hash of the domain block this receipt points to.
    pub domain_hash: DomainHash,
    /// List of storage roots collected during the domain block execution.
    pub trace: Vec<DomainHash>,
    /// The merkle root of `trace`.
    pub trace_root: Blake2b256Hash,
}

impl<Number: Encode, Hash: Encode, DomainNumber: Encode, DomainHash: Encode>
    ExecutionReceipt<Number, Hash, DomainNumber, DomainHash>
{
    /// Returns the hash of this execution receipt.
    pub fn hash(&self) -> H256 {
        BlakeTwo256::hash_of(self)
    }
}

impl<Number: Copy + Zero, Hash, DomainNumber: Zero, DomainHash: Default>
    ExecutionReceipt<Number, Hash, DomainNumber, DomainHash>
{
    #[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
    pub fn dummy(
        consensus_block_number: Number,
        consensus_block_hash: Hash,
    ) -> ExecutionReceipt<Number, Hash, DomainNumber, DomainHash> {
        let trace = if consensus_block_number.is_zero() {
            Vec::new()
        } else {
            sp_std::vec![Default::default(), Default::default()]
        };
        ExecutionReceipt {
            consensus_block_number,
            consensus_block_hash,
            domain_block_number: Zero::zero(),
            domain_hash: Default::default(),
            trace,
            trace_root: Default::default(),
        }
    }

    pub fn genesis(
        consensus_genesis_block_hash: Hash,
    ) -> ExecutionReceipt<Number, Hash, DomainNumber, DomainHash> {
        ExecutionReceipt {
            consensus_block_number: Zero::zero(),
            consensus_block_hash: consensus_genesis_block_hash,
            domain_block_number: Zero::zero(),
            domain_hash: Default::default(),
            trace: Default::default(),
            trace_root: Default::default(),
        }
    }
}

/// List of [`OpaqueBundle`].
pub type OpaqueBundles<Block, DomainNumber, DomainHash> =
    Vec<OpaqueBundle<NumberFor<Block>, <Block as BlockT>::Hash, DomainNumber, DomainHash>>;

#[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
pub fn create_dummy_bundle_with_receipts_generic<BlockNumber, Hash, DomainNumber, DomainHash>(
    domain_id: DomainId,
    consensus_block_number: BlockNumber,
    consensus_block_hash: Hash,
    receipt: ExecutionReceipt<BlockNumber, Hash, DomainNumber, DomainHash>,
) -> OpaqueBundle<BlockNumber, Hash, DomainNumber, DomainHash>
where
    BlockNumber: Encode + Default,
    Hash: Encode + Default,
    DomainHash: Encode + Default,
{
    use sp_core::crypto::UncheckedFrom;

    let sealed_header = SealedBundleHeader {
        header: BundleHeader {
            consensus_block_number,
            consensus_block_hash,
            extrinsics_root: Default::default(),
            proof_of_election: ProofOfElection::dummy(
                domain_id,
                OperatorPublicKey::unchecked_from([0u8; 32]),
            ),
        },
        signature: OperatorSignature::unchecked_from([0u8; 64]),
    };

    OpaqueBundle {
        sealed_header,
        receipt,
        extrinsics: Vec::new(),
    }
}

#[derive(Serialize, Deserialize)]
pub struct GenesisDomain<AccountId> {
    // Domain runtime items
    pub runtime_name: Vec<u8>,
    pub runtime_type: RuntimeType,
    pub runtime_version: RuntimeVersion,
    pub code: Vec<u8>,

    // Domain config items
    pub owner_account_id: AccountId,
    pub domain_name: Vec<u8>,
    pub max_block_size: u32,
    pub max_block_weight: Weight,
    pub bundle_slot_probability: (u64, u64),
    pub target_bundles_per_block: u32,

    // Genesis operator
    pub signing_key: OperatorPublicKey,
    pub minimum_nominator_stake: Balance,
    pub nomination_tax: Percent,
}

/// Types of runtime pallet domains currently supports
#[derive(
    TypeInfo, Debug, Default, Encode, Decode, Clone, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum RuntimeType {
    #[default]
    Evm,
}

impl PassBy for RuntimeType {
    type PassBy = pass_by::Codec<Self>;
}

/// Type representing the runtime ID.
pub type RuntimeId = u32;

/// Type representing domain epoch.
pub type EpochIndex = u32;

/// Type representing operator ID
pub type OperatorId = u64;

/// Domains specific Identifier for Balances freeze.
#[derive(
    PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen, Ord, PartialOrd, Copy, Debug,
)]
pub enum DomainsFreezeIdentifier {
    Staking(OperatorId),
    DomainInstantiation(DomainId),
}

/// Domains specific digest item.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo)]
pub enum DomainDigestItem {
    DomainRuntimeUpgraded(RuntimeId),
}

/// Domains specific digest items.
pub trait DomainsDigestItem {
    fn domain_runtime_upgrade(runtime_id: RuntimeId) -> Self;
    fn as_domain_runtime_upgrade(&self) -> Option<RuntimeId>;
}

impl DomainsDigestItem for DigestItem {
    fn domain_runtime_upgrade(runtime_id: RuntimeId) -> Self {
        Self::Other(DomainDigestItem::DomainRuntimeUpgraded(runtime_id).encode())
    }

    fn as_domain_runtime_upgrade(&self) -> Option<RuntimeId> {
        match self.try_to::<DomainDigestItem>(OpaqueDigestItemId::Other) {
            None => None,
            Some(domain_digest_item) => match domain_digest_item {
                DomainDigestItem::DomainRuntimeUpgraded(runtime_id) => Some(runtime_id),
            },
        }
    }
}

#[cfg(feature = "std")]
pub trait GenerateGenesisStateRoot: Send + Sync {
    /// Returns the state root of genesis block built from the runtime genesis config on success.
    fn generate_genesis_state_root(
        &self,
        runtime_type: RuntimeType,
        raw_runtime_genesis_config: Vec<u8>,
    ) -> Option<H256>;
}

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
    /// A domain genesis receipt extension.
    pub struct GenesisReceiptExtension(std::sync::Arc<dyn GenerateGenesisStateRoot>);
}

#[cfg(feature = "std")]
impl GenesisReceiptExtension {
    /// Create a new instance of [`GenesisReceiptExtension`].
    pub fn new(inner: std::sync::Arc<dyn GenerateGenesisStateRoot>) -> Self {
        Self(inner)
    }
}

/// Domain-related runtime interface
#[runtime_interface]
pub trait Domain {
    fn generate_genesis_state_root(
        &mut self,
        runtime_type: RuntimeType,
        raw_runtime_genesis_config: Vec<u8>,
    ) -> Option<H256> {
        use sp_externalities::ExternalitiesExt;

        self.extension::<GenesisReceiptExtension>()
            .expect("No `GenesisReceiptExtension` associated for the current context!")
            .generate_genesis_state_root(runtime_type, raw_runtime_genesis_config)
    }
}

sp_api::decl_runtime_apis! {
    /// API necessary for domains pallet.
    pub trait DomainsApi<DomainNumber: Encode + Decode, DomainHash: Encode + Decode> {
        /// Submits the transaction bundle via an unsigned extrinsic.
        fn submit_bundle_unsigned(opaque_bundle: OpaqueBundle<NumberFor<Block>, Block::Hash, DomainNumber, DomainHash>);

        /// Extract the bundles stored successfully from the given extrinsics.
        fn extract_successful_bundles(
            extrinsics: Vec<Block::Extrinsic>,
        ) -> OpaqueBundles<Block, DomainNumber, DomainHash>;

        /// Returns the hash of successfully submitted bundles.
        fn successful_bundle_hashes() -> Vec<H256>;

        /// Generates a randomness seed for extrinsics shuffling.
        fn extrinsics_shuffling_seed(header: Block::Header) -> Randomness;

        /// Returns the WASM bundle for given `domain_id`.
        fn domain_runtime_code(domain_id: DomainId) -> Option<Vec<u8>>;

        /// Returns the current timestamp at given height.
        fn timestamp() -> Moment;

        /// Returns the current Tx range for the given domain Id.
        fn domain_tx_range(domain_id: DomainId) -> U256;
    }

    pub trait BundleProducerElectionApi<Balance: Encode + Decode> {
        fn bundle_producer_election_params(domain_id: DomainId) -> Option<BundleProducerElectionParams<Balance>>;

        fn operator_info(operator_id: OperatorId) -> Option<(OperatorPublicKey, Balance)>;
    }
}
