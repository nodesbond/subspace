//! Benchmarking for `pallet-domain-registry`.

// Only enable this module for benchmarking.
#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as DomainRegistry;
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_support::traits::Hooks;
use frame_system::{Pallet as System, RawOrigin};
use schnorrkel::Keypair;
use sp_core::crypto::UncheckedFrom;
use sp_core::H256;
use sp_domain_digests::AsPredigest;
use sp_domains::fraud_proof::{ExecutionPhase, FraudProof, InvalidStateTransitionProof};
use sp_domains::{
    Bundle, BundleHeader, BundleSolution, ExecutionReceipt, ExecutorPublicKey, ExecutorSignature,
    ProofOfElection,
};
use sp_runtime::testing::{Digest, DigestItem};
use sp_runtime::traits::SaturatedConversion;
use sp_runtime::Percent;
use sp_trie::StorageProof;

const SEED: u32 = 0;
const TEST_CORE_DOMAIN_ID: DomainId = DomainId::CORE_PAYMENTS;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn create_domain() {
        let creator = account("creator", 1, SEED);
        let domain_id = NextDomainId::<T>::get();
        let deposit = T::MinDomainDeposit::get();

        let domain_config = sp_domains::DomainConfig {
            wasm_runtime_hash: Default::default(),
            max_bundle_size: 1024 * 1024,
            bundle_slot_probability: (1, 1),
            max_bundle_weight: Weight::MAX,
            min_operator_stake: T::MinDomainOperatorStake::get(),
        };

        T::Currency::make_free_balance_be(&creator, deposit + T::Currency::minimum_balance());

        #[extrinsic_call]
        _(
            RawOrigin::Signed(creator.clone()),
            deposit,
            domain_config.clone(),
        );

        assert_eq!(NextDomainId::<T>::get(), domain_id + 1);
        assert_eq!(Domains::<T>::get(domain_id), Some(domain_config));
        assert_eq!(DomainCreators::<T>::get(domain_id, creator), Some(deposit));
    }

    #[benchmark]
    fn register_domain_operator() {
        let stake = T::MinDomainOperatorStake::get();
        let domain_id = NextDomainId::<T>::get();

        let operator = account("operator", 1, SEED);
        T::Currency::make_free_balance_be(&operator, stake + T::Currency::minimum_balance());

        create_helper_domain::<T>(operator.clone());
        registry_executor::<T>(operator.clone(), T::MinDomainOperatorStake::get());

        #[extrinsic_call]
        _(
            RawOrigin::Signed(operator.clone()),
            domain_id,
            Percent::one(),
        );

        assert_eq!(
            DomainOperators::<T>::get(operator, domain_id),
            Some(Percent::one())
        );
    }

    #[benchmark]
    fn deregister_domain_operator() {
        let stake = T::MinDomainOperatorStake::get();
        let domain_id = NextDomainId::<T>::get();

        let operator = account("operator", 1, SEED);
        T::Currency::make_free_balance_be(&operator, stake + T::Currency::minimum_balance());

        create_helper_domain::<T>(operator.clone());
        registry_executor::<T>(operator.clone(), T::MinDomainOperatorStake::get());

        assert_ok!(DomainRegistry::<T>::do_domain_stake_update(
            operator.clone(),
            domain_id,
            Percent::one()
        ));
        assert_eq!(
            DomainOperators::<T>::get(&operator, domain_id),
            Some(Percent::one())
        );

        #[extrinsic_call]
        _(RawOrigin::Signed(operator.clone()), domain_id);
        assert!(DomainOperators::<T>::get(operator, domain_id).is_none());
    }

    #[benchmark]
    fn submit_core_bundle(x: Linear<1, 256>) {
        let receipts_pruning_depth = T::ReceiptsPruningDepth::get().saturated_into::<u32>();

        // Import `ReceiptsPruningDepth` number of receipts which will be pruned later
        run_to_block::<T>(1, receipts_pruning_depth);
        let receipts: Vec<_> = (0..receipts_pruning_depth)
            .map(create_dummy_receipt::<T>)
            .collect();
        let bundle = create_dummy_bundle_with_receipts_generic(
            TEST_CORE_DOMAIN_ID,
            receipts_pruning_depth.into(),
            Default::default(),
            receipts,
        );
        assert_ok!(DomainRegistry::<T>::submit_core_bundle(
            RawOrigin::None.into(),
            bundle
        ));
        assert_eq!(
            DomainRegistry::<T>::head_receipt_number(TEST_CORE_DOMAIN_ID),
            (receipts_pruning_depth - 1).into()
        );

        // Construct a bundle that contain `x` number of new receipts
        run_to_block::<T>(receipts_pruning_depth + 1, receipts_pruning_depth + x);
        let receipts: Vec<_> = (receipts_pruning_depth..(receipts_pruning_depth + x))
            .map(create_dummy_receipt::<T>)
            .collect();
        let bundle = create_dummy_bundle_with_receipts_generic(
            TEST_CORE_DOMAIN_ID,
            x.into(),
            Default::default(),
            receipts,
        );

        #[extrinsic_call]
        _(RawOrigin::None, bundle);

        assert_eq!(
            DomainRegistry::<T>::head_receipt_number(TEST_CORE_DOMAIN_ID),
            ((receipts_pruning_depth + x) - 1).into()
        );
        assert_eq!(
            DomainRegistry::<T>::oldest_receipt_number(TEST_CORE_DOMAIN_ID),
            x.into()
        );
    }

    #[benchmark]
    fn submit_fraud_proof() {
        let receipts_pruning_depth = T::ReceiptsPruningDepth::get().saturated_into::<u32>();

        // Import `ReceiptsPruningDepth` number of receipts which will be revert later
        run_to_block::<T>(1, receipts_pruning_depth);
        let receipts: Vec<_> = (0..receipts_pruning_depth)
            .map(create_dummy_receipt::<T>)
            .collect();
        let bundle = create_dummy_bundle_with_receipts_generic(
            TEST_CORE_DOMAIN_ID,
            receipts_pruning_depth.into(),
            Default::default(),
            receipts,
        );
        assert_ok!(DomainRegistry::<T>::submit_core_bundle(
            RawOrigin::None.into(),
            bundle
        ));
        assert_eq!(
            DomainRegistry::<T>::head_receipt_number(TEST_CORE_DOMAIN_ID),
            (receipts_pruning_depth - 1).into()
        );

        // Construct a fraud proof that will revert `ReceiptsPruningDepth` number of receipts
        let proof: FraudProof<T::BlockNumber, T::Hash> = FraudProof::InvalidStateTransition(
            dummy_invalid_state_transition_proof(TEST_CORE_DOMAIN_ID, 0),
        );

        #[extrinsic_call]
        _(RawOrigin::None, proof);

        assert_eq!(
            DomainRegistry::<T>::head_receipt_number(TEST_CORE_DOMAIN_ID),
            0u32.into()
        );
    }

    fn create_helper_domain<T: Config>(creator: T::AccountId) {
        let domain_id = NextDomainId::<T>::get();
        let deposit = T::MinDomainDeposit::get();
        let domain_config = sp_domains::DomainConfig {
            wasm_runtime_hash: Default::default(),
            max_bundle_size: 1024 * 1024,
            bundle_slot_probability: (1, 1),
            max_bundle_weight: Weight::MAX,
            min_operator_stake: T::MinDomainOperatorStake::get(),
        };

        T::Currency::make_free_balance_be(&creator, deposit + T::Currency::minimum_balance());

        DomainRegistry::<T>::apply_create_domain(&creator, deposit, &domain_config);
        assert_eq!(NextDomainId::<T>::get(), domain_id + 1);
        assert_eq!(Domains::<T>::get(domain_id), Some(domain_config));
        assert_eq!(DomainCreators::<T>::get(domain_id, creator), Some(deposit));
    }

    fn registry_executor<T: Config>(executor: T::AccountId, stake: BalanceOf<T>) {
        let keypair = Keypair::generate();
        let public_key = ExecutorPublicKey::unchecked_from(keypair.public.to_bytes());

        T::ExecutorRegistry::unchecked_register(executor.clone(), public_key.clone(), stake);

        assert_eq!(T::ExecutorRegistry::executor_stake(&executor), Some(stake));
        assert_eq!(
            T::ExecutorRegistry::executor_public_key(&executor),
            Some(public_key)
        );
    }

    fn block_hash_n<T: Config>(n: u32) -> T::Hash {
        let mut h = T::Hash::default();
        h.as_mut()
            .iter_mut()
            .zip(u32::to_be_bytes(n).as_slice().iter())
            .for_each(|(h, n)| *h = *n);
        h
    }

    fn run_to_block<T: Config>(from: u32, to: u32) {
        assert!(from > 0);
        for b in from..=to {
            let block_number = b.into();
            let hash = block_hash_n::<T>(b - 1);
            let digest = {
                let mut d = Digest::default();
                if b == 1 {
                    d.push(DigestItem::primary_block_info::<T::BlockNumber, _>((
                        0u32.into(),
                        block_hash_n::<T>(b),
                    )));
                }
                d.push(DigestItem::primary_block_info((block_number, hash)));
                d
            };
            System::<T>::set_block_number(block_number);
            System::<T>::initialize(&block_number, &hash, &digest);
            <DomainRegistry<T> as Hooks<T::BlockNumber>>::on_initialize(block_number);
            System::<T>::finalize();
        }
    }

    fn create_dummy_receipt<T: Config>(
        primary_number: u32,
    ) -> ExecutionReceipt<T::BlockNumber, T::Hash, T::DomainHash> {
        ExecutionReceipt {
            primary_number: primary_number.into(),
            primary_hash: block_hash_n::<T>(primary_number),
            domain_hash: Default::default(),
            trace: if primary_number == 0 {
                Vec::new()
            } else {
                vec![Default::default(), Default::default()]
            },
            trace_root: Default::default(),
        }
    }

    fn dummy_invalid_state_transition_proof(
        domain_id: DomainId,
        parent_number: u32,
    ) -> InvalidStateTransitionProof {
        InvalidStateTransitionProof {
            domain_id,
            bad_receipt_hash: H256::default(),
            parent_number,
            primary_parent_hash: H256::default(),
            pre_state_root: H256::default(),
            post_state_root: H256::default(),
            proof: StorageProof::empty(),
            execution_phase: ExecutionPhase::ApplyExtrinsic(0),
        }
    }

    impl_benchmark_test_suite!(
        DomainRegistry,
        crate::tests::new_test_ext(),
        crate::tests::Test
    );
}

pub(crate) fn create_dummy_bundle_with_receipts_generic<BlockNumber, Hash, DomainHash>(
    domain_id: DomainId,
    primary_number: BlockNumber,
    primary_hash: Hash,
    receipts: Vec<ExecutionReceipt<BlockNumber, Hash, DomainHash>>,
) -> SignedOpaqueBundle<BlockNumber, Hash, DomainHash>
where
    BlockNumber: Encode + Default,
    Hash: Encode + Default,
    DomainHash: Encode + Default,
{
    let header = BundleHeader {
        primary_number,
        primary_hash,
        slot_number: 0u64,
        extrinsics_root: Default::default(),
    };

    let bundle = Bundle {
        header,
        receipts,
        extrinsics: Vec::new(),
    };

    let signature = ExecutorSignature::unchecked_from([0u8; 64]);

    let proof_of_election =
        ProofOfElection::dummy(domain_id, ExecutorPublicKey::unchecked_from([0u8; 32]));

    let bundle_solution = if domain_id.is_system() {
        BundleSolution::System {
            authority_stake_weight: Default::default(),
            authority_witness: Default::default(),
            proof_of_election,
        }
    } else if domain_id.is_core() {
        BundleSolution::Core {
            proof_of_election,
            core_block_number: Default::default(),
            core_block_hash: Default::default(),
            core_state_root: Default::default(),
        }
    } else {
        panic!("Open domain unsupported");
    };

    SignedOpaqueBundle {
        bundle,
        bundle_solution,
        signature,
    }
}
