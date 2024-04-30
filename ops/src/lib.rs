use std::time::Instant;

use common::{debug_utils::save_inputs_to_disk, prover_state::p_state};
use evm_arithmetization::proof::PublicValues;
use keccak_hash::keccak;
use paladin::{
    operation::{FatalError, FatalStrategy, Monoid, Operation, Result},
    registry, RemoteExecute,
};
use proof_gen::{
    proof_gen::{generate_block_proof, generate_segment_agg_proof, generate_transaction_agg_proof},
    proof_types::{
        GeneratedBlockProof, GeneratedSegmentProof, GeneratedTxnAggProof, SegmentAggregatableProof,
        TxnAggregatableProof,
    },
};
use serde::{Deserialize, Serialize};
use trace_decoder::types::AllData;
use tracing::{error, event, info_span, Level};

registry!();

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct SegmentProof;

#[cfg(not(feature = "test_only"))]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = proof_gen::proof_types::SegmentAggregatableProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        let _span = SegmentProofSpan::new(&input);
        let proof = if input.1.is_dummy() {
            proof_gen::proof_gen::dummy_proof()
                .map(|p| GeneratedSegmentProof {
                    p_vals: PublicValues::default(),
                    intern: p,
                })
                .map_err(|e| e.into())
        } else {
            common::prover_state::p_manager().generate_segment_proof(input)
        }
        .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate))?;

        Ok(proof.into())
    }
}

#[cfg(feature = "test_only")]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = ();

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        let _span = SegmentProofSpan::new(&input);

        evm_arithmetization::prover::testing::simulate_execution::<proof_gen::types::Field>(
            input.0,
            Some(input.1),
        )
        .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate))?;

        Ok(())
    }
}

/// RAII struct to measure the time taken by a transaction proof.
///
/// - When created, it starts a span with the transaction proof id.
/// - When dropped, it logs the time taken by the transaction proof.
struct SegmentProofSpan {
    _span: tracing::span::EnteredSpan,
    start: Instant,
    descriptor: String,
}

impl SegmentProofSpan {
    /// Get a unique id for the transaction proof.
    fn get_id(ir: &AllData) -> String {
        format!(
            "b{} - {} ({})",
            ir.0.block_metadata.block_number,
            ir.0.txn_number_before,
            ir.1.segment_index(),
        )
    }

    /// Get a textual descriptor for the transaction proof.
    ///
    /// Either the hex-encoded hash of the transaction or "Dummy" if the
    /// transaction is not present.
    fn get_descriptor(ir: &AllData) -> String {
        ir.0.signed_txn
            .as_ref()
            .map(|txn| format!("{:x}", keccak(txn)))
            .unwrap_or_else(|| "Dummy".to_string())
    }

    /// Create a new transaction proof span.
    ///
    /// When dropped, it logs the time taken by the transaction proof.
    fn new(ir: &AllData) -> Self {
        let id = Self::get_id(ir);
        let span = info_span!("p_gen", id).entered();
        let start = Instant::now();
        let descriptor = Self::get_descriptor(ir);
        Self {
            _span: span,
            start,
            descriptor,
        }
    }
}

impl Drop for SegmentProofSpan {
    fn drop(&mut self) {
        event!(
            Level::INFO,
            "segment proof ({}) took {:?}",
            self.descriptor,
            self.start.elapsed()
        );
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct SegmentAggProof;

fn get_seg_agg_proof_public_values(elem: &SegmentAggregatableProof) -> PublicValues {
    match elem {
        SegmentAggregatableProof::Txn(info) => info.p_vals.clone(),
        SegmentAggregatableProof::Agg(info) => info.p_vals.clone(),
    }
}

impl Monoid for SegmentAggProof {
    type Elem = SegmentAggregatableProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        // TODO Robin: Check dummy directly in public values
        let result = generate_segment_agg_proof(
            p_state(),
            &a,
            &b,
            get_seg_agg_proof_public_values(&b).registers_before
                == get_seg_agg_proof_public_values(&b).registers_after,
        )
        .map_err(|e| FatalError::from(e))?;

        Ok(result.into())
    }

    fn empty(&self) -> Self::Elem {
        // Expect that empty blocks are padded.
        unimplemented!("empty agg proof")
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct TxnAggProof {
    pub save_inputs_on_error: bool,
}

fn get_txn_agg_proof_public_values(elem: TxnAggregatableProof) -> PublicValues {
    match elem {
        TxnAggregatableProof::Txn(info) => info.p_vals,
        TxnAggregatableProof::Agg(info) => info.p_vals,
    }
}

impl Monoid for TxnAggProof {
    type Elem = TxnAggregatableProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        let result = generate_transaction_agg_proof(p_state(), &a, &b).map_err(|e| {
            if self.save_inputs_on_error {
                let pv = vec![
                    get_txn_agg_proof_public_values(a),
                    get_txn_agg_proof_public_values(b),
                ];
                if let Err(write_err) = save_inputs_to_disk(
                    format!(
                        "b{}_agg_lhs_rhs_inputs.log",
                        pv[0].block_metadata.block_number
                    ),
                    pv,
                ) {
                    error!("Failed to save agg proof inputs to disk: {:?}", write_err);
                }
            }

            FatalError::from(e)
        })?;

        Ok(result.into())
    }

    fn empty(&self) -> Self::Elem {
        // Expect that empty blocks are padded.
        unimplemented!("empty agg proof")
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct BlockProof {
    pub prev: Option<GeneratedBlockProof>,
    pub save_inputs_on_error: bool,
}

impl Operation for BlockProof {
    type Input = GeneratedTxnAggProof;
    type Output = GeneratedBlockProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        Ok(
            generate_block_proof(p_state(), self.prev.as_ref(), &input).map_err(|e| {
                if self.save_inputs_on_error {
                    if let Err(write_err) = save_inputs_to_disk(
                        format!(
                            "b{}_block_input.log",
                            input.p_vals.block_metadata.block_number
                        ),
                        input.p_vals,
                    ) {
                        error!("Failed to save block proof input to disk: {:?}", write_err);
                    }
                }

                FatalError::from(e)
            })?,
        )
    }
}
