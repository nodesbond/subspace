//! Benchmarking for `pallet-subspace`.

// Only enable this module for benchmarking.
#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Subspace;
use frame_benchmarking::v2::*;
use frame_system::{Pallet as System, RawOrigin};
use schnorrkel::Keypair;
use sp_consensus_subspace::digests::{CompatibleDigestItem, PreDigest};
use sp_consensus_subspace::Vote;
use sp_core::crypto::UncheckedFrom;
use sp_runtime::testing::{Digest, DigestItem};
use sp_runtime::traits::Header;
use subspace_core_primitives::{
    ArchivedBlockProgress, Blake2b256Hash, LastArchivedBlock, SegmentCommitment, SegmentHeader,
    SegmentIndex, Solution,
};

const SEED: u32 = 0;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn report_equivocation() {
        let proof = generate_equivocation_proof::<T>();

        #[extrinsic_call]
        _(RawOrigin::None, Box::new(proof));
    }

    // TODO: constraint `x`
    #[benchmark]
    fn store_segment_headers(x: Linear<1, 256>) {
        let segment_headers: Vec<SegmentHeader> = (0..x as u64)
            .map(|i| create_segment_header(i.into()))
            .collect();

        #[extrinsic_call]
        _(RawOrigin::None, segment_headers);
    }

    #[benchmark]
    fn enable_solution_range_adjustment() {
        #[extrinsic_call]
        _(RawOrigin::Root, Some(1), None);

        assert!(ShouldAdjustSolutionRange::<T>::get());

        let solution_range = SolutionRanges::<T>::get();
        assert_eq!(solution_range.current, 1);
        assert_eq!(
            solution_range.voting_current,
            u64::from(T::ExpectedVotesPerBlock::get()) + 1
        );

        let next_solution_range_override = NextSolutionRangeOverride::<T>::get()
            .expect("NextSolutionRangeOverride should be filled");
        assert_eq!(next_solution_range_override.solution_range, 1);
        assert_eq!(
            next_solution_range_override.voting_solution_range,
            u64::from(T::ExpectedVotesPerBlock::get()) + 1
        );
    }

    #[benchmark]
    fn vote() {
        let keypair = Keypair::generate();
        let reward_signing_context = schnorrkel::signing_context(REWARD_SIGNING_CONTEXT);
        let unsigned_vote: Vote<T::BlockNumber, T::Hash, T::AccountId> = Vote::V0 {
            height: System::<T>::block_number(),
            parent_hash: System::<T>::parent_hash(),
            slot: CurrentSlot::<T>::get(),
            solution: Solution::genesis_solution(
                FarmerPublicKey::unchecked_from(keypair.public.to_bytes()),
                account("user1", 1, SEED),
            ),
        };
        let signature = FarmerSignature::unchecked_from(
            keypair
                .sign(reward_signing_context.bytes(unsigned_vote.hash().as_ref()))
                .to_bytes(),
        );
        let signed_vote = SignedVote {
            vote: unsigned_vote,
            signature,
        };

        #[extrinsic_call]
        _(RawOrigin::None, Box::new(signed_vote));
    }

    #[benchmark]
    fn enable_rewards() {
        EnableRewards::<T>::take();

        #[extrinsic_call]
        _(RawOrigin::Root, Some(100u32.into()));

        assert_eq!(EnableRewards::<T>::get(), Some(100u32.into()));
    }

    #[benchmark]
    fn enable_storage_access() {
        #[extrinsic_call]
        _(RawOrigin::Root);

        assert!(Subspace::<T>::is_storage_access_enabled());
    }

    #[benchmark]
    fn enable_authoring_by_anyone() {
        #[extrinsic_call]
        _(RawOrigin::Root);

        assert!(AllowAuthoringByAnyone::<T>::get());
        assert!(Subspace::<T>::root_plot_public_key().is_none());
    }

    fn create_segment_header(segment_index: SegmentIndex) -> SegmentHeader {
        SegmentHeader::V0 {
            segment_index,
            segment_commitment: SegmentCommitment::default(),
            prev_segment_header_hash: Blake2b256Hash::default(),
            last_archived_block: LastArchivedBlock {
                number: 0,
                archived_progress: ArchivedBlockProgress::Complete,
            },
        }
    }

    fn generate_equivocation_proof<T: Config>(
    ) -> sp_consensus_subspace::EquivocationProof<T::Header> {
        let slot = CurrentSlot::<T>::get();
        let keypair = Keypair::generate();
        let public_key = FarmerPublicKey::unchecked_from(keypair.public.to_bytes());

        let make_header = |reward_address: <T as frame_system::Config>::AccountId| {
            let digest = {
                let log = DigestItem::subspace_pre_digest(&PreDigest {
                    slot,
                    solution: Solution::genesis_solution(public_key.clone(), reward_address),
                });
                Digest { logs: vec![log] }
            };
            T::Header::new(
                System::<T>::block_number(),
                Default::default(),
                Default::default(),
                Default::default(),
                digest,
            )
        };
        let seal_header = |header: &mut T::Header| {
            let prehash = header.hash();
            let signature = FarmerSignature::unchecked_from(
                keypair
                    .sign(
                        schnorrkel::context::signing_context(
                            subspace_solving::REWARD_SIGNING_CONTEXT,
                        )
                        .bytes(prehash.as_ref()),
                    )
                    .to_bytes(),
            );
            let seal = DigestItem::subspace_seal(signature);
            header.digest_mut().push(seal);
        };

        let mut h1 = make_header(account("user0", 0, SEED));
        let mut h2 = make_header(account("user1", 1, SEED));

        seal_header(&mut h1);
        seal_header(&mut h2);

        sp_consensus_subspace::EquivocationProof {
            slot,
            offender: public_key,
            first_header: h1,
            second_header: h2,
        }
    }

    impl_benchmark_test_suite!(Subspace, crate::mock::new_test_ext(), crate::mock::Test);
}
