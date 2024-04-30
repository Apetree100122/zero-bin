use anyhow::Result;
use ethereum_types::U256;
use paladin::{
    directive::{Directive, IndexedStream},
    runtime::Runtime,
};
use proof_gen::{proof_types::GeneratedBlockProof, types::PlonkyProofIntern};
use serde::{Deserialize, Serialize};
use trace_decoder::{
    processed_block_trace::ProcessingMeta,
    trace_protocol::BlockTrace,
    types::{CodeHash, OtherBlockData},
};
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct ProverInput {
    pub block_trace: BlockTrace,
    pub other_data: OtherBlockData,
}
fn resolve_code_hash_fn(_: &CodeHash) -> Vec<u8> {
    todo!()
}

impl ProverInput {
    pub fn get_block_number(&self) -> U256 {
        self.other_data.b_data.b_meta.block_number
    }

    #[cfg(not(feature = "test_only"))]
    pub async fn prove(
        self,
        runtime: &Runtime,
        max_cpu_len_log: usize,
        previous: Option<PlonkyProofIntern>,
    ) -> Result<GeneratedBlockProof> {
        use evm_arithmetization::prover::{generate_all_data_segments, GenerationSegmentData};
        use futures::{stream::FuturesUnordered, FutureExt};
        use ops::SegmentProof;
        use plonky2::field::goldilocks_field::GoldilocksField;

        let block_number = self.get_block_number();
        info!("Proving block {block_number}");

        let other_data = self.other_data;
        let txs = self.block_trace.into_txn_proof_gen_ir(
            &ProcessingMeta::new(resolve_code_hash_fn),
            other_data.clone(),
        )?;

        // Generate segment data.
        type F = GoldilocksField;

        // Map the transactions to a stream of transaction proofs.
        let tx_proof_futs: FuturesUnordered<_> = txs
            .into_iter()
            .enumerate()
            .map(|(idx, txn)| {
                let generated_data = generate_all_data_segments::<F>(Some(max_cpu_len_log), &txn)
                    .unwrap_or(vec![GenerationSegmentData::default()]);
                info!("generated data length {:?}", generated_data.len());

                let cur_data: Vec<_> = generated_data
                    .into_iter()
                    .map(|d| (txn.clone(), d))
                    .collect();

                Directive::map(IndexedStream::from(cur_data), &SegmentProof)
                    .fold(&ops::SegmentAggProof)
                    .run(runtime)
                    .map(move |e| {
                        e.map(|p| (idx, proof_gen::proof_types::TxnAggregatableProof::from(p)))
                    })
            })
            .collect();

        // Fold the transaction proof stream into a single transaction proof.
        let final_txn_proof = Directive::fold(IndexedStream::new(tx_proof_futs), &ops::TxnAggProof)
            .run(runtime)
            .await?;

        if let proof_gen::proof_types::TxnAggregatableProof::Agg(proof) = final_txn_proof {
            let prev = previous.map(|p| GeneratedBlockProof {
                b_height: block_number.as_u64() - 1,
                intern: p,
            });

            let block_proof = paladin::directive::Literal(proof)
                .map(&ops::BlockProof { prev })
                .run(runtime)
                .await?;

            info!("Successfully proved block {block_number}");

            Ok(block_proof.0)
        } else {
            anyhow::bail!("AggProof is is not GeneratedAggProof")
        }
    }

    #[cfg(feature = "test_only")]
    pub async fn prove(
        self,
        runtime: &Runtime,
        max_cpu_len_log: usize,
        _previous: Option<PlonkyProofIntern>,
    ) -> Result<GeneratedBlockProof> {
        use evm_arithmetization::prover::{generate_all_data_segments, GenerationSegmentData};
        use futures::stream::FuturesOrdered;
        use ops::SegmentProof;
        use plonky2::field::goldilocks_field::GoldilocksField;

        let block_number = self.get_block_number();
        info!("Testing witness generation for block {block_number}.");

        let other_data = self.other_data;
        let txs = self.block_trace.into_txn_proof_gen_ir(
            &ProcessingMeta::new(resolve_code_hash_fn),
            other_data.clone(),
        )?;

        type F = GoldilocksField;
        let mut cur_data = vec![];
        let tx_proof_futs: FuturesOrdered<_> = txs
            .iter()
            .map(|txn| {
                let generated_data =
                    generate_all_data_segments::<F>(Some(max_cpu_len_log), txn.clone())
                        .unwrap_or(vec![GenerationSegmentData::default()]);
                info!("Generated all data");
                cur_data = generated_data
                    .iter()
                    .map(|d| (txn.clone(), max_cpu_len_log, d.clone()))
                    .collect();
                IndexedStream::from(cur_data.clone())
                    .map(&SegmentProof)
                    .run(runtime)
            })
            .collect();

        let _ = TryStreamExt::try_collect::<Vec<_>>(tx_proof_futs).await?;

        info!("Successfully generated witness for block {block_number}.");

        // Dummy proof to match expected output type.
        Ok(GeneratedBlockProof {
            b_height: block_number.as_u64(),
            intern: proof_gen::proof_gen::dummy_proof()?,
        })
    }
}
