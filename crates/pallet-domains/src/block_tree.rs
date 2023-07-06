//! Domain block tree

use crate::{
    BlockTree, Config, DomainBlocks, ExecutionInbox, ExecutionReceiptOf, HeadReceiptNumber,
};
use codec::{Decode, Encode};
use frame_support::{ensure, PalletError};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_domains::v2::ExecutionReceipt;
use sp_domains::{DomainId, OperatorId};
use sp_runtime::traits::{CheckedSub, One, Saturating, Zero};
use sp_std::cmp::Ordering;
use sp_std::vec::Vec;

/// Block tree specific errors
#[derive(TypeInfo, Encode, Decode, PalletError, Debug, PartialEq)]
pub enum Error {
    InvalidExtrinsicsRoots,
    UnknownParentBlockReceipt,
    BuiltOnUnknownConsensusBlock,
    ChallengeGenesisReceipt,
    ExceedMaxBlockTreeFork,
    UnexpectedReceiptType,
    MaxHeadDomainNumber,
}

#[derive(TypeInfo, Debug, Encode, Decode, Clone, PartialEq, Eq)]
pub struct DomainBlock<Number, Hash, DomainNumber, DomainHash, Balance> {
    /// The full ER for this block.
    pub execution_receipt: ExecutionReceipt<Number, Hash, DomainNumber, DomainHash, Balance>,
    /// A set of all operators who have committed to this ER within a bundle. Used to determine who to
    /// slash if a fraudulent branch of the `block_tree` is pruned.
    ///
    /// NOTE: there may be duplicated operator id as an operator can submit multiple bundles with the
    /// same head receipt to a consensus block.
    pub operator_ids: Vec<OperatorId>,
}

/// The type of receipt regarding to its freshness
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReceiptType {
    // New head receipt that extend the longest branch
    NewHead,
    // Receipt that comfirm the current head receipt
    CurrentHead,
    // Receipt that create a new branch of the block tree
    NewBranch,
    // Receipt that newer than the head receipt but not extend the head receipt
    InFuture,
    // Receipt that comfirm a non-head receipt
    Stale,
    // Receipt that already been pruned
    Pruned,
}

/// Get the receipt type of the given receipt based on the current block tree state
pub(crate) fn execution_receipt_type<T: Config>(
    domain_id: DomainId,
    execution_receipt: &ExecutionReceiptOf<T>,
) -> ReceiptType {
    let receipt_number = execution_receipt.domain_block_height;
    let head_receipt_number = HeadReceiptNumber::<T>::get(domain_id);

    match receipt_number.cmp(&head_receipt_number.saturating_add(One::one())) {
        Ordering::Greater => ReceiptType::InFuture,
        Ordering::Equal => ReceiptType::NewHead,
        Ordering::Less => {
            let oldest_receipt_number =
                head_receipt_number.saturating_sub(T::BlockTreePruningDepth::get());
            let already_exist =
                BlockTree::<T>::get(domain_id, receipt_number).contains(&execution_receipt.hash());

            if receipt_number < oldest_receipt_number {
                // Receipt alreay pruned
                ReceiptType::Pruned
            } else if !already_exist {
                // Create new branch
                ReceiptType::NewBranch
            } else if receipt_number == head_receipt_number {
                // Add comfirm to the current head receipt
                ReceiptType::CurrentHead
            } else {
                // Add comfirm to a non-head receipt
                ReceiptType::Stale
            }
        }
    }
}

/// Verify the execution receipt
pub(crate) fn verify_execution_receipt<T: Config>(
    domain_id: DomainId,
    execution_receipt: &ExecutionReceiptOf<T>,
) -> Result<(), Error> {
    let ExecutionReceipt {
        consensus_block_height,
        consensus_block_hash,
        domain_block_height,
        block_extrinsics_roots,
        parent_domain_block_receipt,
        ..
    } = execution_receipt;

    if !domain_block_height.is_zero() {
        let execution_inbox = ExecutionInbox::<T>::get(domain_id, domain_block_height);
        ensure!(
            !block_extrinsics_roots.is_empty() && *block_extrinsics_roots == execution_inbox,
            Error::InvalidExtrinsicsRoots
        );
    }

    let excepted_consensus_block_hash =
        frame_system::Pallet::<T>::block_hash(consensus_block_height);
    ensure!(
        *consensus_block_hash == excepted_consensus_block_hash,
        Error::BuiltOnUnknownConsensusBlock
    );

    if let Some(parent_block_height) = domain_block_height.checked_sub(&One::one()) {
        let parent_block_exist = BlockTree::<T>::get(domain_id, parent_block_height)
            .contains(parent_domain_block_receipt);
        ensure!(parent_block_exist, Error::UnknownParentBlockReceipt);
    }

    match execution_receipt_type::<T>(domain_id, execution_receipt) {
        ReceiptType::InFuture => unreachable!("In future receipt should result in `UnknownParentBlockReceipt` error as it parent receipt is missing"),
        ReceiptType::Pruned => unreachable!(
            "Pruned receipt should result in `InvalidExtrinsicsRoots` error as its `ExecutionInbox` is pruned at the same time"
        ),
        // The genesis receipt is generated and added to the block tree by the runtime upon domain
        // instantiation, thus it is unchallengeable and must always be the same.
        ReceiptType::NewBranch if domain_block_height.is_zero() => {
            Err(Error::ChallengeGenesisReceipt)
        }
        ReceiptType::NewHead
        | ReceiptType::NewBranch
        | ReceiptType::CurrentHead
        | ReceiptType::Stale => Ok(()),
    }
}

/// Process the execution receipt to add it to the block tree
///
/// NOTE: only `NewHead`, `NewBranch` and `CurrentHead` type of receipt is expected
/// for this function, passing other type of receipt will result in an `UnexpectedReceiptType`
/// error.
pub(crate) fn process_execution_receipt<T: Config>(
    domain_id: DomainId,
    submitter: OperatorId,
    execution_receipt: ExecutionReceiptOf<T>,
    receipt_type: ReceiptType,
) -> Result<(), Error> {
    let er_hash = execution_receipt.hash();
    match receipt_type {
        er_type @ ReceiptType::NewHead | er_type @ ReceiptType::NewBranch => {
            // Construct and add a new domain block to the block tree
            let domain_block_height = execution_receipt.domain_block_height;
            let domain_block = DomainBlock {
                execution_receipt,
                operator_ids: sp_std::vec![submitter],
            };
            BlockTree::<T>::mutate(domain_id, domain_block_height, |er_hashes| {
                er_hashes
                    .try_insert(er_hash)
                    .map_err(|_| Error::ExceedMaxBlockTreeFork)?;
                Ok(())
            })?;
            DomainBlocks::<T>::insert(er_hash, domain_block);

            if er_type == ReceiptType::NewHead {
                // Update the head receipt number
                HeadReceiptNumber::<T>::insert(domain_id, domain_block_height);

                // Prune expired domain block
                if let Some(to_prune) =
                    domain_block_height.checked_sub(&T::BlockTreePruningDepth::get())
                {
                    for block in BlockTree::<T>::take(domain_id, to_prune) {
                        DomainBlocks::<T>::remove(block);
                    }
                    // Remove the block's `ExecutionInbox` as the block is pruned and not need
                    // to verify the its receipt's `extrinsics_root` anymore
                    ExecutionInbox::<T>::remove(domain_id, to_prune);
                }
            }
        }
        ReceiptType::CurrentHead => {
            // Add confirmation to the current head receipt
            DomainBlocks::<T>::mutate(er_hash, |maybe_domain_block| {
                let domain_block = maybe_domain_block.as_mut().expect(
                    "The domain block of `CurrentHead` receipt is checked to be exist in `execution_receipt_type`; qed"
                );
                domain_block.operator_ids.push(submitter);
            });
        }
        // Other types of receipt is unexpected for this function
        _ => return Err(Error::UnexpectedReceiptType),
    }
    Ok(())
}

/// Import the genesis receipt to the block tree
pub(crate) fn import_genesis_receipt<T: Config>(
    domain_id: DomainId,
    genesis_receipt: ExecutionReceiptOf<T>,
) {
    let er_hash = genesis_receipt.hash();
    let domain_block_height = genesis_receipt.domain_block_height;
    let domain_block = DomainBlock {
        execution_receipt: genesis_receipt,
        operator_ids: sp_std::vec![],
    };
    // NOTE: no need to upate the head receipt number as we are using `ValueQuery`
    BlockTree::<T>::mutate(domain_id, domain_block_height, |er_hashes| {
        er_hashes.try_insert(er_hash)
            .expect(
                "Must not exceed MaxBlockTreeFork as the genesis receipt is the first and only receipt at block #0; qed"
            );
    });
    DomainBlocks::<T>::insert(er_hash, domain_block);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_registry::DomainConfig;
    use crate::tests::{
        create_dummy_bundle_with_receipts, create_dummy_receipt, new_test_ext,
        GenesisStateRootGenerater, ReadRuntimeVersion, Test,
    };
    use crate::NextDomainId;
    use frame_support::dispatch::RawOrigin;
    use frame_support::traits::{Currency, Hooks};
    use frame_support::weights::Weight;
    use frame_support::{assert_err, assert_ok};
    use frame_system::Pallet as System;
    use sp_core::H256;
    use sp_domains::{GenesisReceiptExtension, RuntimeType};
    use sp_runtime::traits::BlockNumberProvider;
    use sp_version::RuntimeVersion;
    use std::sync::Arc;

    fn run_to_block<T: Config>(block_number: T::BlockNumber, parent_hash: T::Hash) {
        System::<T>::set_block_number(block_number);
        System::<T>::initialize(&block_number, &parent_hash, &Default::default());
        <crate::Pallet<T> as Hooks<T::BlockNumber>>::on_initialize(block_number);
        System::<T>::finalize();
    }

    #[test]
    fn test_block_tree() {
        let creator = 1u64;
        let operator_id = 1u64;
        let block_tree_pruning_depth = <Test as Config>::BlockTreePruningDepth::get() as u64;

        let version = RuntimeVersion {
            spec_name: "test".into(),
            impl_name: Default::default(),
            authoring_version: 0,
            spec_version: 1,
            impl_version: 1,
            apis: Default::default(),
            transaction_version: 1,
            state_version: 0,
        };

        let domain_config = DomainConfig {
            domain_name: b"evm-domain".to_vec(),
            runtime_id: 0,
            max_block_size: 1u32,
            max_block_weight: Weight::from_parts(1, 0),
            bundle_slot_probability: (1, 1),
            target_bundles_per_block: 1,
        };

        let mut ext = new_test_ext();
        ext.register_extension(sp_core::traits::ReadRuntimeVersionExt::new(
            ReadRuntimeVersion(version.encode()),
        ));
        ext.register_extension(GenesisReceiptExtension::new(Arc::new(
            GenesisStateRootGenerater,
        )));
        ext.execute_with(|| {
            assert_ok!(crate::Pallet::<Test>::register_domain_runtime(
                RawOrigin::Root.into(),
                b"evm".to_vec(),
                RuntimeType::Evm,
                vec![1, 2, 3, 4],
            ));

            let domain_id = NextDomainId::<Test>::get();
            <Test as Config>::Currency::make_free_balance_be(
                &creator,
                <Test as Config>::DomainInstantiationDeposit::get(),
            );
            assert_ok!(crate::Pallet::<Test>::instantiate_domain(
                RawOrigin::Signed(creator).into(),
                domain_config,
            ));

            // The genesis receipt should be added to the block tree
            let block_tree_node_at_0 = BlockTree::<Test>::get(domain_id, 0);
            assert_eq!(block_tree_node_at_0.len(), 1);

            let genesis_node =
                DomainBlocks::<Test>::get(block_tree_node_at_0.first().unwrap()).unwrap();
            assert!(genesis_node.operator_ids.is_empty());
            assert_eq!(HeadReceiptNumber::<Test>::get(domain_id), 0);

            // The genesis receipt should be able pass the verification and is unchallengeable
            let genesis_receipt = genesis_node.execution_receipt;
            let invalid_genesis_receipt = {
                let mut receipt = genesis_receipt.clone();
                receipt.final_state_root = H256::random();
                receipt
            };
            assert_ok!(verify_execution_receipt::<Test>(
                domain_id,
                &genesis_receipt
            ));
            assert_err!(
                verify_execution_receipt::<Test>(domain_id, &invalid_genesis_receipt),
                Error::ChallengeGenesisReceipt
            );

            let mut receipt_of_block_1 = None;
            let mut receipt = genesis_receipt;
            for block_number in 1..=(block_tree_pruning_depth + 3) {
                // Run to `block_number`
                run_to_block::<Test>(
                    block_number,
                    frame_system::Pallet::<Test>::block_hash(block_number - 1),
                );

                // Submit a bundle with the receipt of the last block
                let bundle_extrinsics_root = H256::random();
                let bundle = create_dummy_bundle_with_receipts(
                    domain_id,
                    block_number,
                    operator_id,
                    bundle_extrinsics_root,
                    receipt,
                );
                assert_ok!(crate::Pallet::<Test>::submit_bundle_v2(
                    RawOrigin::None.into(),
                    bundle,
                ));
                // `bundle_extrinsics_root` should be tracked in `ExecutionInbox`
                assert_eq!(
                    ExecutionInbox::<Test>::get(domain_id, block_number),
                    vec![bundle_extrinsics_root]
                );

                // Head receipt number should be updated
                let head_receipt_number = HeadReceiptNumber::<Test>::get(domain_id);
                assert_eq!(head_receipt_number, block_number - 1);

                // As we only extending the block tree there should be no fork
                let parent_block_tree_nodes =
                    BlockTree::<Test>::get(domain_id, head_receipt_number);
                assert_eq!(parent_block_tree_nodes.len(), 1);

                // The submitter is should be added to `operator_ids`
                let parent_domain_block_receipt = parent_block_tree_nodes.first().unwrap();
                let parent_node = DomainBlocks::<Test>::get(parent_domain_block_receipt).unwrap();
                assert_eq!(parent_node.operator_ids.len(), 1);
                assert_eq!(parent_node.operator_ids[0], operator_id);

                // Construct a `NewHead` receipt of the just submitted bundle, which will be included in the next bundle
                receipt = create_dummy_receipt(
                    block_number,
                    frame_system::Pallet::<Test>::block_hash(block_number),
                    *parent_domain_block_receipt,
                    vec![bundle_extrinsics_root],
                );
                assert_eq!(
                    execution_receipt_type::<Test>(domain_id, &receipt),
                    ReceiptType::NewHead
                );
                assert_ok!(verify_execution_receipt::<Test>(domain_id, &receipt));
                if block_number == 1 {
                    receipt_of_block_1.replace(receipt.clone());
                }
            }

            let get_block_tree_node_at = |height| {
                DomainBlocks::<Test>::get(
                    BlockTree::<Test>::get(domain_id, height).first().unwrap(),
                )
                .unwrap()
            };
            let head_receipt_number = HeadReceiptNumber::<Test>::get(domain_id);
            let current_block_number = frame_system::Pallet::<Test>::current_block_number();

            // The receipt of the block 1 is pruned at the last iteration, verify it will result in
            // `InvalidExtrinsicsRoots` error as `ExecutionInbox` of block 1 is pruned
            let pruned_receipt = receipt_of_block_1.unwrap();
            assert!(BlockTree::<Test>::get(domain_id, 1).is_empty());
            assert!(ExecutionInbox::<Test>::get(domain_id, 1).is_empty());
            assert_eq!(
                execution_receipt_type::<Test>(domain_id, &pruned_receipt),
                ReceiptType::Pruned
            );
            assert_err!(
                verify_execution_receipt::<Test>(domain_id, &pruned_receipt),
                Error::InvalidExtrinsicsRoots
            );

            // Receipt that comfirm the head receipt
            let operator_id2 = 2u64;
            let current_head_receipt =
                get_block_tree_node_at(head_receipt_number).execution_receipt;
            assert_eq!(
                execution_receipt_type::<Test>(domain_id, &current_head_receipt),
                ReceiptType::CurrentHead
            );
            assert_ok!(verify_execution_receipt::<Test>(
                domain_id,
                &current_head_receipt
            ));
            let bundle = create_dummy_bundle_with_receipts(
                domain_id,
                current_block_number,
                operator_id2,
                H256::random(),
                current_head_receipt.clone(),
            );
            assert_ok!(crate::Pallet::<Test>::submit_bundle_v2(
                RawOrigin::None.into(),
                bundle,
            ));
            let head_node = get_block_tree_node_at(head_receipt_number);
            assert_eq!(head_node.operator_ids, vec![operator_id, operator_id2]);

            // Receipt that comfirm a non-head receipt is stale receipt but can pass `verify_execution_receipt`
            let stale_receipt = get_block_tree_node_at(head_receipt_number - 1).execution_receipt;
            assert_eq!(
                execution_receipt_type::<Test>(domain_id, &stale_receipt),
                ReceiptType::Stale
            );
            assert_ok!(verify_execution_receipt::<Test>(domain_id, &stale_receipt));

            // Receipt that fork away from an existing node of the block tree
            let mut new_branch_receipt = current_head_receipt.clone();
            new_branch_receipt.final_state_root = H256::random();
            assert_eq!(
                execution_receipt_type::<Test>(domain_id, &new_branch_receipt),
                ReceiptType::NewBranch
            );
            assert_ok!(verify_execution_receipt::<Test>(
                domain_id,
                &new_branch_receipt
            ));
            let bundle = create_dummy_bundle_with_receipts(
                domain_id,
                current_block_number,
                operator_id2,
                H256::random(),
                new_branch_receipt,
            );
            assert_ok!(crate::Pallet::<Test>::submit_bundle_v2(
                RawOrigin::None.into(),
                bundle,
            ));
            let block_tree_tip = BlockTree::<Test>::get(domain_id, head_receipt_number);
            assert_eq!(block_tree_tip.len(), 2);

            // In future receipt will result in `UnknownParentBlockReceipt` error as its parent
            // receipt is missing from the block tree
            let mut future_receipt = current_head_receipt.clone();
            future_receipt.domain_block_height = head_receipt_number + 2;
            ExecutionInbox::<Test>::insert(
                domain_id,
                head_receipt_number + 2,
                future_receipt.block_extrinsics_roots.clone(),
            );
            assert_eq!(
                execution_receipt_type::<Test>(domain_id, &future_receipt),
                ReceiptType::InFuture
            );
            assert_err!(
                verify_execution_receipt::<Test>(domain_id, &future_receipt),
                Error::UnknownParentBlockReceipt
            );

            // Receipt droven from unknown consensus block
            let mut unknown_consensus_block_receipt = current_head_receipt.clone();
            unknown_consensus_block_receipt.consensus_block_hash = H256::random();
            assert_err!(
                verify_execution_receipt::<Test>(domain_id, &unknown_consensus_block_receipt),
                Error::BuiltOnUnknownConsensusBlock
            );

            // Receipt with unknown parent receipt
            let mut unknown_parent_receipt = current_head_receipt;
            unknown_parent_receipt.parent_domain_block_receipt = H256::random();
            assert_err!(
                verify_execution_receipt::<Test>(domain_id, &unknown_parent_receipt),
                Error::UnknownParentBlockReceipt
            );
        });
    }
}
