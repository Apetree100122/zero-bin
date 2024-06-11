use std::future::Future;

use alloy::primitives::U256;
use anyhow::Result;
use common::prover_state::p_state;
use futures::{future::BoxFuture, stream::FuturesOrdered, FutureExt, TryFutureExt, TryStreamExt};
use num_traits::ToPrimitive as _;
use paladin::{
    directive::{Directive, IndexedStream},
    runtime::Runtime,
};
use proof_gen::proof_gen::generate_segment_agg_proof;
use proof_gen::proof_types::GeneratedBlockProof;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use trace_decoder::{
    processed_block_trace::ProcessingMeta,
    trace_protocol::BlockTrace,
    types::{CodeHash, OtherBlockData},
};
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockProverInput {
    pub block_trace: BlockTrace,
    pub other_data: OtherBlockData,
}
fn resolve_code_hash_fn(_: &CodeHash) -> Vec<u8> {
    todo!()
}

impl BlockProverInput {
    pub fn get_block_number(&self) -> U256 {
        self.other_data.b_data.b_meta.block_number.into()
    }

    #[cfg(not(feature = "test_only"))]
    pub async fn prove(
        self,
        runtime: &Runtime,
        max_cpu_len_log: usize,
        previous: Option<impl Future<Output = Result<GeneratedBlockProof>>>,
        batch_size: usize,
        save_inputs_on_error: bool,
    ) -> Result<GeneratedBlockProof> {
        use anyhow::Context as _;
        use evm_arithmetization::prover::SegmentDataIterator;
        use futures::{stream::FuturesUnordered, FutureExt};
        use ops::SegmentProof;
        use proof_gen::proof_types::SegmentAggregatableProof;

        let block_number = self.get_block_number();
        info!("Proving block {block_number}");

        let other_data = self.other_data;
        let txs = self.block_trace.into_txn_proof_gen_ir(
            &ProcessingMeta::new(resolve_code_hash_fn),
            other_data.clone(),
            batch_size,
        )?;

        // Generate segment data.

        let seg_ops = SegmentProof {
            save_inputs_on_error,
        };

        let agg_ops = ops::SegmentAggProof {
            save_inputs_on_error,
        };

        // Map the transactions to a stream of transaction proofs.
        let tx_proof_futs: FuturesUnordered<_> = txs
            .into_iter()
            .enumerate()
            .map(|(idx, txn)| {
                let data_iterator = SegmentDataIterator {
                    partial_next_data: None,
                    inputs: txn,
                    max_cpu_len_log: Some(max_cpu_len_log),
                };

                Directive::map(IndexedStream::from(data_iterator), &seg_ops)
                    .fold(&agg_ops)
                    .run(runtime)
                    .map(move |e| {
                        e.map(|p| match p {
                            SegmentAggregatableProof::Agg(_) => {
                                (idx, proof_gen::proof_types::TxnAggregatableProof::from(p))
                            }
                            SegmentAggregatableProof::Seg(seg) => {
                                let segment_proof = seg.into();
                                let single_aggreg = generate_segment_agg_proof(
                                    p_state(),
                                    &segment_proof,
                                    &segment_proof,
                                    true,
                                )
                                .expect("Single-segment aggregation failed?");
                                (
                                    idx,
                                    proof_gen::proof_types::TxnAggregatableProof::from(
                                        single_aggreg,
                                    ),
                                )
                            }
                        })
                    })
            })
            .collect();

        // Fold the transaction proof stream into a single transaction proof.
        let final_txn_proof = Directive::fold(
            IndexedStream::new(tx_proof_futs),
            &ops::TxnAggProof {
                save_inputs_on_error,
            },
        )
        .run(runtime)
        .await?;

        if let proof_gen::proof_types::TxnAggregatableProof::Agg(proof) = final_txn_proof {
            let block_number = block_number
                .to_u64()
                .context("block number overflows u64")?;
            let prev = match previous {
                Some(it) => Some(it.await?),
                None => None,
            };

            let block_proof = paladin::directive::Literal(proof)
                .map(&ops::BlockProof {
                    prev,
                    save_inputs_on_error,
                })
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
        _runtime: &Runtime,
        max_cpu_len_log: usize,
        _previous: Option<impl Future<Output = Result<GeneratedBlockProof>>>,
        batch_size: usize,
        save_inputs_on_error: bool,
    ) -> Result<GeneratedBlockProof> {
        use evm_arithmetization::prover::testing::simulate_all_segments_interpreter;
        use plonky2::field::goldilocks_field::GoldilocksField;

        let block_number = self.get_block_number();
        info!("Testing witness generation for block {block_number}.");

        let other_data = self.other_data;
        let txs = self.block_trace.into_txn_proof_gen_ir(
            &ProcessingMeta::new(resolve_code_hash_fn),
            other_data.clone(),
            batch_size,
        )?;

        type F = GoldilocksField;
        for txn in txs.into_iter() {
            simulate_all_segments_interpreter::<F>(txn, max_cpu_len_log)?;
        }

        info!("Successfully generated witness for block {block_number}.");

        // Dummy proof to match expected output type.
        Ok(GeneratedBlockProof {
            b_height: block_number
                .to_u64()
                .expect("Block number should fit in a u64"),
            intern: proof_gen::proof_gen::dummy_proof()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProverInput {
    pub blocks: Vec<BlockProverInput>,
}

impl ProverInput {
    pub async fn prove(
        self,
        runtime: &Runtime,
        max_cpu_len_log: usize,
        previous_proof: Option<GeneratedBlockProof>,
        batch_size: usize,
        save_inputs_on_error: bool,
    ) -> Result<Vec<GeneratedBlockProof>> {
        let mut prev: Option<BoxFuture<Result<GeneratedBlockProof>>> =
            previous_proof.map(|proof| Box::pin(futures::future::ok(proof)) as BoxFuture<_>);

        let results: FuturesOrdered<_> = self
            .blocks
            .into_iter()
            .map(|block| {
                let block_number = block.get_block_number();
                info!("Proving block {block_number}");

                let (tx, rx) = oneshot::channel::<GeneratedBlockProof>();

                // Prove the block
                let fut = block
                    .prove(
                        runtime,
                        max_cpu_len_log,
                        prev.take(),
                        batch_size,
                        save_inputs_on_error,
                    )
                    .then(|proof| async {
                        let proof = proof?;

                        if tx.send(proof.clone()).is_err() {
                            anyhow::bail!("Failed to send proof");
                        }

                        Ok(proof)
                    })
                    .boxed();

                prev = Some(Box::pin(rx.map_err(anyhow::Error::new)));

                fut
            })
            .collect();

        results.try_collect().await
    }
}
