use std::time::Instant;

use common::prover_state::p_state;
use keccak_hash::keccak;
use paladin::{
    operation::{FatalError, FatalStrategy, Monoid, Operation, Result},
    registry, RemoteExecute,
};
use proof_gen::{
    proof_gen::{generate_block_proof, generate_segment_agg_proof, generate_transaction_agg_proof},
    proof_types::{
        GeneratedBlockProof, GeneratedTxnAggProof, SegmentAggregatableProof, TxnAggregatableProof,
    },
};
use serde::{Deserialize, Serialize};
use trace_decoder::types::AllData;
use tracing::{event, info_span, Level};

registry!();

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct SegmentProof;

fn run_and_wrap_txn_proof_in_elapsed_span<F, O>(f: F, ident: String) -> Result<O>
where
    F: Fn() -> Result<O>,
{
    let _span = info_span!("proof generation", ident).entered();
    let start = Instant::now();

    let proof = f()?;

    event!(
        Level::INFO,
        "txn proof {:.4} took {:?}",
        ident,
        start.elapsed()
    );
    Ok(proof)
}

#[cfg(not(feature = "test_only"))]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = proof_gen::proof_types::SegmentAggregatableProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        let seg_ident = Self::seg_ident(&input);

        let proof = run_and_wrap_txn_proof_in_elapsed_span(
            || {
                common::prover_state::p_manager()
                    .generate_segment_proof(input.clone())
                    .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate).into())
            },
            seg_ident,
        )?;

        Ok(proof.into())
    }
}

#[cfg(feature = "test_only")]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = ();

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        let seg_ident = Self::seg_ident(&input);

        run_and_wrap_txn_proof_in_elapsed_span(
            || {
                evm_arithmetization::prover::testing::simulate_execution::<proof_gen::types::Field>(
                    input.0.clone(),
                )
                .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate).into())
            },
            seg_ident,
        )?;

        Ok(())
    }
}

impl SegmentProof {
    fn seg_ident(all_data: &AllData) -> String {
        let ir = &all_data.0;
        let txn_hash_str = if ir.signed_txns.is_empty() {
            "Dummy".to_string()
        } else {
            format!("{:x}", keccak(ir.signed_txns[0].clone()))
        };
        format!(
            "b{} - {} ({})",
            ir.block_metadata.block_number, ir.txn_number_before, txn_hash_str
        )
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct SegmentAggProof;

impl Monoid for SegmentAggProof {
    type Elem = SegmentAggregatableProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        let result =
            generate_segment_agg_proof(p_state(), &a, &b, false).map_err(FatalError::from)?;

        Ok(result.into())
    }

    fn empty(&self) -> Self::Elem {
        // Expect that empty blocks are padded.
        unimplemented!("empty agg proof")
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct TxnAggProof;

impl Monoid for TxnAggProof {
    type Elem = TxnAggregatableProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        let lhs = match a {
            TxnAggregatableProof::Segment(segment) => TxnAggregatableProof::from(
                generate_segment_agg_proof(
                    p_state(),
                    &SegmentAggregatableProof::from(segment.clone()),
                    &SegmentAggregatableProof::from(segment),
                    true,
                )
                .map_err(FatalError::from)?,
            ),
            _ => a,
        };

        let rhs = match b {
            TxnAggregatableProof::Segment(segment) => TxnAggregatableProof::from(
                generate_segment_agg_proof(
                    p_state(),
                    &SegmentAggregatableProof::from(segment.clone()),
                    &SegmentAggregatableProof::from(segment),
                    true,
                )
                .map_err(FatalError::from)?,
            ),
            _ => b,
        };
        let result =
            generate_transaction_agg_proof(p_state(), &lhs, &rhs).map_err(FatalError::from)?;

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
}

impl Operation for BlockProof {
    type Input = GeneratedTxnAggProof;
    type Output = GeneratedBlockProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        Ok(
            generate_block_proof(p_state(), self.prev.as_ref(), &input)
                .map_err(FatalError::from)?,
        )
    }
}
