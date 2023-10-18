use crate::fraud_proof::{ProofDataPerExpectedInvalidBundle, TrueInvalidBundleEntryFraudProof};
use crate::{ExecutionReceipt, DOMAIN_EXTRINSICS_SHUFFLING_SEED_SUBJECT};
use domain_runtime_primitives::opaque::AccountId;
use frame_support::PalletError;
use hash_db::Hasher;
use parity_scale_codec::{Decode, Encode};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scale_info::TypeInfo;
use sp_api::BlockT;
use sp_core::storage::StorageKey;
use sp_runtime::traits::{BlakeTwo256, Block, NumberFor};
use sp_runtime::OpaqueExtrinsic;
use sp_state_machine::trace;
use sp_std::collections::btree_map::BTreeMap;
use sp_std::collections::vec_deque::VecDeque;
use sp_std::fmt::Debug;
use sp_std::marker::PhantomData;
use sp_std::vec::Vec;
use sp_trie::{read_trie_value, LayoutV1, StorageProof};
use subspace_core_primitives::Randomness;

/// Verification error.
#[derive(Debug, PartialEq, Eq, Encode, Decode, PalletError, TypeInfo)]
pub enum VerificationError {
    /// Emits when the given storage proof is invalid.
    InvalidProof,
    /// Value doesn't exist in the Db for the given key.
    MissingValue,
    /// Failed to decode value.
    FailedToDecode,
    /// Invalid bundle digest
    InvalidBundleDigest,
    /// Bundle with requested index not found in execution receipt
    BundleNotFound,
}

pub struct StorageProofVerifier<H: Hasher>(PhantomData<H>);

impl<H: Hasher> StorageProofVerifier<H> {
    pub fn verify_and_get_value<V: Decode>(
        state_root: &H::Out,
        proof: StorageProof,
        key: StorageKey,
    ) -> Result<V, VerificationError> {
        let db = proof.into_memory_db::<H>();
        let val = read_trie_value::<LayoutV1<H>, _>(&db, state_root, key.as_ref(), None, None)
            .map_err(|_| VerificationError::InvalidProof)?
            .ok_or(VerificationError::MissingValue)?;

        let decoded = V::decode(&mut &val[..]).map_err(|_| VerificationError::FailedToDecode)?;

        Ok(decoded)
    }
}

pub fn verify_true_invalid_bundle_fraud_proof<CBlock, DomainNumber, DomainHash, Balance>(
    bad_receipt: ExecutionReceipt<
        NumberFor<CBlock>,
        CBlock::Hash,
        DomainNumber,
        DomainHash,
        Balance,
    >,
    true_invalid_fraud_proof: &TrueInvalidBundleEntryFraudProof,
) -> Result<(), VerificationError>
where
    CBlock: BlockT,
{
    let true_invalid_bundle_entry = bad_receipt
        .inboxed_bundles
        .get(true_invalid_fraud_proof.bundle_index as usize)
        .ok_or(VerificationError::BundleNotFound)?;

    let _extrinsic: OpaqueExtrinsic = StorageProofVerifier::<BlakeTwo256>::verify_and_get_value(
        &true_invalid_bundle_entry.extrinsics_root,
        StorageProof::new(
            true_invalid_fraud_proof
                .extrinsic_inclusion_proof
                .clone()
                .drain(..),
        ),
        StorageKey(
            true_invalid_fraud_proof
                .mismatched_extrinsic_index
                .to_be_bytes()
                .to_vec(),
        ),
    )?;

    match true_invalid_fraud_proof.proof_data {
        ProofDataPerExpectedInvalidBundle::OutOfRangeTx {} => {
            // TODO: Replace this with actual invocation of host functions
            Ok(())
        }
    }
}

pub fn verify_invalid_total_rewards_fraud_proof<
    CBlock,
    DomainNumber,
    DomainHash,
    Balance,
    Hashing,
>(
    bad_receipt: ExecutionReceipt<
        NumberFor<CBlock>,
        CBlock::Hash,
        DomainNumber,
        DomainHash,
        Balance,
    >,
    storage_proof: &StorageProof,
) -> Result<(), VerificationError>
where
    CBlock: Block,
    Balance: PartialEq + Decode,
    Hashing: Hasher<Out = CBlock::Hash>,
    DomainHash: Encode,
{
    let state_root = bad_receipt.final_state_root.encode();
    let state_root = CBlock::Hash::decode(&mut state_root.as_slice())
        .map_err(|_| VerificationError::FailedToDecode)?;
    let storage_key = StorageKey(crate::fraud_proof::operator_block_rewards_final_key());
    let storage_proof = storage_proof.clone();

    let total_rewards = StorageProofVerifier::<Hashing>::verify_and_get_value::<Balance>(
        &state_root,
        storage_proof,
        storage_key,
    )
    .map_err(|_| VerificationError::InvalidProof)?;

    // if the rewards matches, then this is an invalid fraud proof since rewards must be different.
    if bad_receipt.total_rewards == total_rewards {
        return Err(VerificationError::InvalidProof);
    }

    Ok(())
}

pub fn extrinsics_shuffling_seed<Hashing>(block_randomness: Randomness) -> Hashing::Out
where
    Hashing: Hasher,
{
    let mut subject = DOMAIN_EXTRINSICS_SHUFFLING_SEED_SUBJECT.to_vec();
    subject.extend_from_slice(block_randomness.as_ref());
    Hashing::hash(&subject)
}

pub fn deduplicate_and_shuffle_extrinsics<Extrinsic>(
    mut extrinsics: Vec<(Option<AccountId>, Extrinsic)>,
    shuffling_seed: Randomness,
) -> VecDeque<Extrinsic>
where
    Extrinsic: Debug + PartialEq + Clone,
{
    let mut seen = Vec::new();
    extrinsics.retain(|(_, uxt)| match seen.contains(uxt) {
        true => {
            trace!(extrinsic = ?uxt, "Duplicated extrinsic");
            false
        }
        false => {
            seen.push(uxt.clone());
            true
        }
    });
    drop(seen);
    trace!(?extrinsics, "Origin deduplicated extrinsics");
    shuffle_extrinsics::<Extrinsic, AccountId>(extrinsics, shuffling_seed)
}

/// Shuffles the extrinsics in a deterministic way.
///
/// The extrinsics are grouped by the signer. The extrinsics without a signer, i.e., unsigned
/// extrinsics, are considered as a special group. The items in different groups are cross shuffled,
/// while the order of items inside the same group is still maintained.
pub fn shuffle_extrinsics<Extrinsic: Debug, AccountId: Ord + Clone>(
    extrinsics: Vec<(Option<AccountId>, Extrinsic)>,
    shuffling_seed: Randomness,
) -> VecDeque<Extrinsic> {
    let mut rng = ChaCha8Rng::from_seed(*shuffling_seed);

    let mut positions = extrinsics
        .iter()
        .map(|(maybe_signer, _)| maybe_signer)
        .cloned()
        .collect::<Vec<_>>();

    // Shuffles the positions using Fisher–Yates algorithm.
    positions.shuffle(&mut rng);

    let mut grouped_extrinsics: BTreeMap<Option<AccountId>, VecDeque<_>> = extrinsics
        .into_iter()
        .fold(BTreeMap::new(), |mut groups, (maybe_signer, tx)| {
            groups.entry(maybe_signer).or_default().push_back(tx);
            groups
        });

    // The relative ordering for the items in the same group does not change.
    let shuffled_extrinsics = positions
        .into_iter()
        .map(|maybe_signer| {
            grouped_extrinsics
                .get_mut(&maybe_signer)
                .expect("Extrinsics are grouped correctly; qed")
                .pop_front()
                .expect("Extrinsic definitely exists as it's correctly grouped above; qed")
        })
        .collect::<VecDeque<_>>();

    trace!(?shuffled_extrinsics, "Shuffled extrinsics");

    shuffled_extrinsics
}
