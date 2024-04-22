use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
};

use anyhow::Result;
use ethereum_types::H256;
use paladin::runtime::Runtime;
use proof_gen::types::PlonkyProofIntern;

use crate::cli::BlockNumbers;

/// The main function for the jerigon mode.
pub(crate) async fn jerigon_main(
    runtime: Runtime,
    rpc_url: &str,
    block_numbers: BlockNumbers,
    checkpoint_block_number: u64,
    previous: Option<PlonkyProofIntern>,
    proof_output_dir_opt: Option<PathBuf>,
) -> Result<()> {
    let mut previous = previous;
    let mut block_hash_cache: HashMap<u64, H256> = HashMap::new();

    match block_numbers {
        BlockNumbers::Single(block_number) => {
            process_block(
                &runtime,
                rpc_url,
                block_number,
                checkpoint_block_number,
                previous,
                &proof_output_dir_opt,
                &mut block_hash_cache,
            )
            .await?;
        }
        BlockNumbers::RangeInclusive(block_numbers_range) => {
            // Code to execute if BlockNumbers is RangeInclusive
            for block_number in block_numbers_range {
                previous = process_block(
                    &runtime,
                    rpc_url,
                    block_number,
                    checkpoint_block_number,
                    previous,
                    &proof_output_dir_opt,
                    &mut block_hash_cache,
                )
                .await?;
            }
        }
    }

    runtime.close().await?;

    Ok(())
}

async fn process_block(
    runtime: &Runtime,
    rpc_url: &str,
    block_number: u64,
    checkpoint_block_number: u64,
    previous: Option<PlonkyProofIntern>,
    proof_output_dir_opt: &Option<PathBuf>,
    block_hash_cache: &mut HashMap<u64, H256>,
) -> Result<Option<PlonkyProofIntern>> {
    let prover_input = rpc::fetch_prover_input(rpc::FetchProverInputRequest {
        rpc_url,
        block_number,
        checkpoint_block_number,
        block_hash_cache,
    })
    .await?;

    let proof = prover_input.prove(runtime, previous).await?;

    let proof_json = serde_json::to_vec(&proof.intern)?;
    write_proof(
        proof_json,
        proof_output_dir_opt
            .as_ref()
            .map(|p| p.join(format!("b{}.zkproof", block_number))),
    )?;

    Ok(Some(proof.intern))
}

fn write_proof(proof: Vec<u8>, proof_output_path_opt: Option<PathBuf>) -> Result<()> {
    match proof_output_path_opt {
        Some(p) => {
            if let Some(parent) = p.parent() {
                create_dir_all(parent)?;
            }

            let mut f = File::create(p)?;
            f.write_all(&proof)?;
        }
        None => std::io::stdout().write_all(&proof)?,
    }

    Ok(())
}
