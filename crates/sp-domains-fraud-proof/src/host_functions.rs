use crate::{FraudProofVerificationInfoRequest, FraudProofVerificationInfoResponse};
use codec::{Decode, Encode};
use domain_block_preprocessor::runtime_api::InherentExtrinsicConstructor;
use domain_block_preprocessor::runtime_api_light::RuntimeApiLight;
use sc_client_api::BlockBackend;
use sc_executor::RuntimeVersionOf;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::traits::CodeExecutor;
use sp_core::H256;
use sp_domains::{DomainId, DomainsApi};
use sp_runtime::traits::{Header, NumberFor};
use sp_runtime::OpaqueExtrinsic;
use std::marker::PhantomData;
use std::sync::Arc;
use subspace_core_primitives::{Randomness, U256};

/// Trait to query and verify Domains Fraud proof.
pub trait FraudProofHostFunctions: Send + Sync {
    /// Returns the required verification info for the runtime to verify the Fraud proof.
    fn get_fraud_proof_verification_info(
        &self,
        consensus_block_hash: H256,
        fraud_proof_verification_req: FraudProofVerificationInfoRequest,
    ) -> Option<FraudProofVerificationInfoResponse>;
}

sp_externalities::decl_extension! {
    /// Domains fraud proof host function
    pub struct FraudProofExtension(std::sync::Arc<dyn FraudProofHostFunctions>);
}

impl FraudProofExtension {
    /// Create a new instance of [`FraudProofExtension`].
    pub fn new(inner: std::sync::Arc<dyn FraudProofHostFunctions>) -> Self {
        Self(inner)
    }
}

/// Trait Impl to query and verify Domains Fraud proof.
pub struct FraudProofHostFunctionsImpl<Block, Client, DomainBlock, Executor> {
    consensus_client: Arc<Client>,
    executor: Arc<Executor>,
    _phantom: PhantomData<(Block, DomainBlock)>,
}

impl<Block, Client, DomainBlock, Executor>
    FraudProofHostFunctionsImpl<Block, Client, DomainBlock, Executor>
{
    pub fn new(consensus_client: Arc<Client>, executor: Arc<Executor>) -> Self {
        FraudProofHostFunctionsImpl {
            consensus_client,
            executor,
            _phantom: Default::default(),
        }
    }
}

impl<Block, Client, DomainBlock, Executor>
    FraudProofHostFunctionsImpl<Block, Client, DomainBlock, Executor>
where
    Block: BlockT,
    Block::Hash: From<H256>,
    DomainBlock: BlockT,
    Client: BlockBackend<Block> + HeaderBackend<Block> + ProvideRuntimeApi<Block>,
    Client::Api: DomainsApi<Block, NumberFor<DomainBlock>, DomainBlock::Hash>,
    Executor: CodeExecutor + RuntimeVersionOf,
{
    fn get_block_randomness(&self, consensus_block_hash: H256) -> Option<Randomness> {
        let runtime_api = self.consensus_client.runtime_api();
        let consensus_block_hash = consensus_block_hash.into();
        runtime_api
            .extrinsics_shuffling_seed(consensus_block_hash)
            .ok()
    }

    fn derive_domain_timestamp_extrinsic(
        &self,
        consensus_block_hash: H256,
        domain_id: DomainId,
    ) -> Option<Vec<u8>> {
        let runtime_api = self.consensus_client.runtime_api();
        let consensus_block_hash = consensus_block_hash.into();
        let runtime_code = runtime_api
            .domain_runtime_code(consensus_block_hash, domain_id)
            .ok()??;
        let timestamp = runtime_api.timestamp(consensus_block_hash).ok()?;

        let domain_runtime_api_light =
            RuntimeApiLight::new(self.executor.clone(), runtime_code.into());

        InherentExtrinsicConstructor::<DomainBlock>::construct_timestamp_inherent_extrinsic(
            &domain_runtime_api_light,
            // We do not care about the domain hash since this is stateless call into
            // domain runtime,
            Default::default(),
            timestamp,
        )
        .ok()
        .map(|ext| ext.encode())
    }

    fn is_tx_in_range(
        &self,
        consensus_block_hash_with_bundle: H256,
        consensus_block_hash_with_tx_range: H256,
        domain_id: DomainId,
        opaque_extrinsic: OpaqueExtrinsic,
        bundle_index: u32,
    ) -> Option<bool> {
        let runtime_api = self.consensus_client.runtime_api();
        let consensus_block_hash_with_tx_range = consensus_block_hash_with_tx_range.into();
        let domain_tx_range = runtime_api
            .domain_tx_range(consensus_block_hash_with_tx_range, domain_id)
            .ok()?;

        let consensus_block_hash_with_bundles = consensus_block_hash_with_bundle.into();
        let consensus_extrinsics = self
            .consensus_client
            .block_body(consensus_block_hash_with_bundles)
            .ok()??;
        let bundles = self
            .consensus_client
            .runtime_api()
            .extract_successful_bundles(
                consensus_block_hash_with_bundles,
                domain_id,
                consensus_extrinsics,
            )
            .ok()?;

        let bundle = bundles.get(bundle_index as usize)?;
        let bundle_vrf_hash =
            U256::from_be_bytes(bundle.sealed_header.header.proof_of_election.vrf_hash());

        // Currently, Runtime code of previous consensus block is used to derive bundles.
        // TODO: Change this when current consensus block contains runtime used to derive bundles.
        let consensus_block_header_with_runtime_code = self
            .consensus_client
            .header(consensus_block_hash_with_bundles)
            .ok()??;
        let consensus_block_hash_with_runtime_code =
            consensus_block_header_with_runtime_code.parent_hash();
        let runtime_code = runtime_api
            .domain_runtime_code(*consensus_block_hash_with_runtime_code, domain_id)
            .ok()??;

        let domain_runtime_api_light =
            RuntimeApiLight::new(self.executor.clone(), runtime_code.into());

        let encoded_extrinsic = opaque_extrinsic.encode();
        let extrinsic =
            <DomainBlock as BlockT>::Extrinsic::decode(&mut encoded_extrinsic.as_slice()).ok()?;

        <RuntimeApiLight<Executor> as domain_runtime_primitives::DomainCoreApi<
                DomainBlock,
            >>::is_within_tx_range(
                &domain_runtime_api_light,
                Default::default(), // Doesn't matter for RuntimeApiLight
                &extrinsic,
                &bundle_vrf_hash,
                &domain_tx_range,
            ).ok()
    }
}

impl<Block, Client, DomainBlock, Executor> FraudProofHostFunctions
    for FraudProofHostFunctionsImpl<Block, Client, DomainBlock, Executor>
where
    Block: BlockT,
    Block::Hash: From<H256>,
    DomainBlock: BlockT,
    Client: BlockBackend<Block> + HeaderBackend<Block> + ProvideRuntimeApi<Block>,
    Client::Api: DomainsApi<Block, NumberFor<DomainBlock>, DomainBlock::Hash>,
    Executor: CodeExecutor + RuntimeVersionOf,
{
    fn get_fraud_proof_verification_info(
        &self,
        consensus_block_hash: H256,
        fraud_proof_verification_req: FraudProofVerificationInfoRequest,
    ) -> Option<FraudProofVerificationInfoResponse> {
        match fraud_proof_verification_req {
            FraudProofVerificationInfoRequest::BlockRandomness => self
                .get_block_randomness(consensus_block_hash)
                .map(|block_randomness| {
                    FraudProofVerificationInfoResponse::BlockRandomness(block_randomness)
                }),
            FraudProofVerificationInfoRequest::DomainTimestampExtrinsic(doman_id) => self
                .derive_domain_timestamp_extrinsic(consensus_block_hash, doman_id)
                .map(|domain_timestamp_extrinsic| {
                    FraudProofVerificationInfoResponse::DomainTimestampExtrinsic(
                        domain_timestamp_extrinsic,
                    )
                }),
            FraudProofVerificationInfoRequest::TxRangeCheck {
                consensus_block_hash_with_tx_range,
                domain_id,
                opaque_extrinsic,
                bundle_index,
                ..
            } => self
                .is_tx_in_range(
                    consensus_block_hash,
                    consensus_block_hash_with_tx_range,
                    domain_id,
                    opaque_extrinsic,
                    bundle_index,
                )
                .map(|is_tx_in_range| {
                    FraudProofVerificationInfoResponse::TxRangeCheck(is_tx_in_range)
                }),
        }
    }
}
