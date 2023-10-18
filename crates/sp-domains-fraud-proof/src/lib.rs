// Copyright (C) 2022 Subspace Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Subspace fraud proof primitives for consensus chain.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod host_functions;
mod runtime_interface;
pub mod verification;

use codec::{Decode, Encode};
#[cfg(feature = "std")]
pub use host_functions::{
    FraudProofExtension, FraudProofHostFunctions, FraudProofHostFunctionsImpl,
};
pub use runtime_interface::fraud_proof_runtime_interface;
#[cfg(feature = "std")]
pub use runtime_interface::fraud_proof_runtime_interface::HostFunctions;
use sp_api::scale_info::TypeInfo;
use sp_core::H256;
use sp_domains::DomainId;
use sp_runtime::OpaqueExtrinsic;
use sp_runtime_interface::pass_by;
use sp_runtime_interface::pass_by::PassBy;
use sp_std::vec::Vec;
use subspace_core_primitives::Randomness;

/// Request type to fetch required verification information for fraud proof through Host function.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub enum FraudProofVerificationInfoRequest {
    /// Block randomness at a given consensus block hash.
    BlockRandomness,
    /// Domain timestamp extrinsic using the timestamp at a given consensus block hash.
    DomainTimestampExtrinsic(DomainId),
    /// Request to check if particular extrinsic is in range for (domain, bundle) pair at given domain block
    TxRangeCheck {
        domain_id: DomainId,
        /// Hash of the consensus block at which tx_range was queried
        consensus_block_hash_with_tx_range: H256,
        /// State root of domain state at particular block. (Unused since currently api is stateless)
        domain_block_state_root: H256,
        /// Runtime storage with proof required for executing tx range check. (Unused since currently api is stateless)
        domain_runtime_storage_proof: Vec<Vec<u8>>,
        /// Index of the bundle in which the extrinsic exists
        bundle_index: u32,
        /// Extrinsic for which we need to check the range
        opaque_extrinsic: OpaqueExtrinsic,
    },
}

impl PassBy for FraudProofVerificationInfoRequest {
    type PassBy = pass_by::Codec<Self>;
}

/// Response holds required verification information for fraud proof from Host function.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub enum FraudProofVerificationInfoResponse {
    /// Block randomness fetched from consensus state at a specific block hash.
    BlockRandomness(Randomness),
    /// Encoded domain timestamp extrinsic using the timestamp from consensus state at a specific block hash.
    DomainTimestampExtrinsic(Vec<u8>),
    /// if particular extrinsic is in range for (domain, bundle) pair at given domain block
    TxRangeCheck(bool),
}
