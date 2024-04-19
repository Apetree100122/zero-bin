#!/bin/bash

# Args:
# 1 --> Start block idx
# 2 --> End block index (inclusive)
# 3 --> Rpc endpoint:port (eg. http://35.246.1.96:8545)
# 4 --> Ignore previous proofs (boolean)

export RUST_BACKTRACE=1
export RUST_LOG=mpt_trie=info,trace_decoder=info,plonky2=info,evm_arithmetization=trace,leader=info
export RUSTFLAGS='-Ctarget-cpu=native'

export ARITHMETIC_CIRCUIT_SIZE="16..23"
export BYTE_PACKING_CIRCUIT_SIZE="9..21"
export CPU_CIRCUIT_SIZE="12..25"
export KECCAK_CIRCUIT_SIZE="14..20"
export KECCAK_SPONGE_CIRCUIT_SIZE="9..15"
export LOGIC_CIRCUIT_SIZE="12..18"
export MEMORY_CIRCUIT_SIZE="17..28"

PROOF_OUTPUT_DIR="proofs"
ALWAYS_WRITE_LOGS=0 # Change this to `1` if you always want logs to be written.

TOT_BLOCKS=$(($2-$1+1))
IGNORE_PREVIOUS_PROOFS=$4

echo "Proving blocks ${1}..${2} (Total: ${TOT_BLOCKS})"
mkdir -p $PROOF_OUTPUT_DIR

OUT_LOG_PATH="${PROOF_OUTPUT_DIR}/b_from_${1}_to_${2}.log"
err_msg="Blocks $1..$2 errored. See ${OUT_LOG_PATH} for more details."
prev_proof_num=$(($1-1))

if [ $1 -eq $2 ]; then
    OUT_LOG_PATH="${PROOF_OUTPUT_DIR}/b$1.log"
    err_msg="Block $1 errored. See ${OUT_LOG_PATH} for more details."
fi

if [ $IGNORE_PREVIOUS_PROOFS ]; then
    # Set checkpoint height to previous block number
    PREV_PROOF_EXTRA_ARG="--checkpoint-block-number ${prev_proof_num}"
else
    if [ $1 -gt 1 ]; then
        PREV_PROOF_EXTRA_ARG="-f ${PROOF_OUTPUT_DIR}/b${prev_proof_num}.zkproof"
    fi
fi

cargo r --release --bin leader -- --runtime in-memory jerigon --rpc-url "$3" --from $1 --to $2 --proof-output-dir $PROOF_OUTPUT_DIR $PREV_PROOF_EXTRA_ARG > $OUT_LOG_PATH 2>&1

retVal=$?
echo $retVal
if [ $retVal -ne 0 ]; then
    # Some error occured.
    echo $err_msg
    exit $retVal
else
    # Remove the log on success if we don't want to keep it.
    if [ $ALWAYS_WRITE_LOGS -ne 1 ]; then
        rm $OUT_LOG_PATH
    fi
fi

echo "Successfully generated ${TOT_BLOCKS} proofs!"