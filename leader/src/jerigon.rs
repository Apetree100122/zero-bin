use std::{
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

    let block_range = match block_numbers {
        BlockNumbers::Single(block_number) => block_number..=block_number,
        BlockNumbers::RangeInclusive(block_range) => block_range,
    };

    let mut prev_hashes = rpc::fetch_previous_block_hashes(rpc_url, *block_range.start()).await?;

    for block_number in block_range {
        let (curr_hash, curr_proof) = process_block(
            &runtime,
            rpc_url,
            block_number,
            checkpoint_block_number,
            previous,
            &prev_hashes,
            &proof_output_dir_opt,
        )
        .await?;

        previous = curr_proof;
        prev_hashes.remove(0);
        prev_hashes.push(curr_hash);
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
    prev_hashes: &Vec<H256>,
    proof_output_dir_opt: &Option<PathBuf>,
) -> Result<(H256, Option<PlonkyProofIntern>)> {
    let prover_input = rpc::fetch_prover_input(rpc::FetchProverInputRequest {
        rpc_url,
        block_number,
        checkpoint_block_number,
        prev_hashes,
    })
    .await?;

    let curr_hash = prover_input.other_data.b_data.b_hashes.cur_hash.clone();
    let proof = prover_input.prove(runtime, previous).await?;

    let proof_json = serde_json::to_vec(&proof.intern)?;
    write_proof(
        proof_json,
        proof_output_dir_opt
            .as_ref()
            .map(|p| p.join(format!("b{}.zkproof", block_number))),
    )?;

    Ok((curr_hash, Some(proof.intern)))
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
