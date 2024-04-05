use common::prover_state::p_state;
use paladin::{
    operation::{FatalError, FatalStrategy, Monoid, Operation, Result},
    registry, RemoteExecute,
};
use proof_gen::{
    proof_gen::{generate_agg_proof, generate_block_proof, generate_transaction_agg_proof},
    proof_types::{
        AggregatableProof, AggregatableTxnProof, GeneratedAggProof, GeneratedBlockProof,
    },
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

fn generate_txn_agg_proof(
    a: AggregatableTxnProof,
    b: AggregatableTxnProof,
) -> Result<AggregatableTxnProof> {
    match a {
        AggregatableTxnProof::Txn(_) => Err(paladin::operation::OperationError::Fatal {
            err: core::fmt::Error.into(),
            strategy: FatalStrategy::Terminate,
        }),
        AggregatableTxnProof::Agg(block) => match b {
            AggregatableTxnProof::Txn(agg) => {
                let proof = generate_transaction_agg_proof(p_state(), block.as_ref(), &agg)
                    .map_err(FatalError::from)?;
                Ok(AggregatableTxnProof::Agg(Some(proof)))
            }
            AggregatableTxnProof::Agg(_) => Err(paladin::operation::OperationError::Fatal {
                err: core::fmt::Error.into(),
                strategy: FatalStrategy::Terminate,
            }),
        },
    }
}
#[derive(Deserialize, Serialize, RemoteExecute)]
pub struct FullTxnProof;

impl Monoid for FullTxnProof {
    type Elem = AggregatableTxnProof;

    fn combine(&self, a: Self::Elem, b: Self::Elem) -> Result<Self::Elem> {
        Ok(generate_txn_agg_proof(a, b).map_err(FatalError::from)?)
    }

    fn empty(&self) -> Self::Elem {
        // Expect that empty blocks are padded.
        unimplemented!("empty txn agg proof")
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
