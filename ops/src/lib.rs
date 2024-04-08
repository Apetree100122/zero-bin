use common::prover_state::p_state;
use paladin::{
    operation::{FatalError, FatalStrategy, Monoid, Operation, Result},
    registry, RemoteExecute,
};
use proof_gen::{
    proof_gen::{generate_agg_proof, generate_block_proof, generate_transaction_agg_proof},
    proof_types::{AggregatableProof, GeneratedAggProof, GeneratedBlockProof},
};
use serde::{Deserialize, Serialize};
use trace_decoder::types::AllData;

registry!();

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct SegmentProof;

#[cfg(not(feature = "test_only"))]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = proof_gen::proof_types::AggregatableProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        let proof = common::prover_state::p_manager()
            .generate_txn_proof(input)
            .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate))?;

        Ok(proof.into())
    }
}

#[cfg(feature = "test_only")]
impl Operation for SegmentProof {
    type Input = AllData;
    type Output = ();

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        evm_arithmetization::prover::testing::simulate_execution::<proof_gen::types::Field>(
            input.0,
            Some(input.1),
        )
        .map_err(|err| FatalError::from_anyhow(err, FatalStrategy::Terminate))?;

        Ok(())
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct AggProof;

impl Monoid for AggProof {
    type Elem = AggregatableProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        let result = generate_agg_proof(p_state(), &a, &b).map_err(FatalError::from)?;

        Ok(result.into())
    }

    fn empty(&self) -> Self::Elem {
        // Expect that empty blocks are padded.
        unimplemented!("empty agg proof")
    }
}

pub fn generate_txn_agg_proof(
    a: Option<AggregatableProof>,
    b: AggregatableProof,
) -> Result<AggregatableProof> {
    match (a, b) {
        (None, AggregatableProof::Agg(b_agg)) => {
            let p = generate_transaction_agg_proof(p_state(), None, &b_agg)
                .map_err(|e| paladin::operation::OperationError::from(FatalError::from(e)))?;
            Ok(AggregatableProof::Agg(p))
        }
        (Some(AggregatableProof::Agg(a_agg)), AggregatableProof::Agg(b_agg)) => {
            let p = generate_transaction_agg_proof(p_state(), Some(&a_agg), &b_agg)
                .map_err(|e| paladin::operation::OperationError::from(FatalError::from(e)))?;
            Ok(AggregatableProof::Agg(p))
        }
        _ => panic!("Transaction proofs should be aggregations."),
    }
}

#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct BlockProof {
    pub prev: Option<GeneratedBlockProof>,
}

impl Operation for BlockProof {
    type Input = GeneratedAggProof;
    type Output = GeneratedBlockProof;

    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        Ok(
            generate_block_proof(p_state(), self.prev.as_ref(), &input)
                .map_err(FatalError::from)?,
        )
    }
}
