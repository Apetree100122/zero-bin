#!/bin/bash

# Args:
# 1 --> Start block idx
# 2 --> End block index (inclusive)
# 3 --> Rpc endpoint:port (eg. http://35.246.1.96:8545)
# 4 --> Ignore previous proofs (boolean)
# 5 --> Test run only flag `test_only` (optional)

export RUST_MIN_STACK=33554432
export RUST_BACKTRACE=1
export RUST_LOG=info
# Disable the lld linker for now, as it's causing issues with the linkme package.
# https://github.com/rust-lang/rust/pull/124129
# https://github.com/dtolnay/linkme/pull/88
export RUSTFLAGS='-C target-cpu=native -Zlinker-features=-lld'

if [[ $5 == "test_only" ]]; then
  # Circuit sizes don't matter in test_only mode, so we keep them minimal.
  export ARITHMETIC_CIRCUIT_SIZE="16..17"
  export BYTE_PACKING_CIRCUIT_SIZE="9..10"
  export CPU_CIRCUIT_SIZE="12..13"
  export KECCAK_CIRCUIT_SIZE="14..15"
  export KECCAK_SPONGE_CIRCUIT_SIZE="9..10"
  export LOGIC_CIRCUIT_SIZE="12..13"
  export MEMORY_CIRCUIT_SIZE="17..18"
  export MEM_BEFORE_CIRCUIT_SIZE="8..9"
  export MEM_AFTER_CIRCUIT_SIZE="8..9"
else
  export ARITHMETIC_CIRCUIT_SIZE="16..23"
  export BYTE_PACKING_CIRCUIT_SIZE="9..21"
  export CPU_CIRCUIT_SIZE="12..25"
  export KECCAK_CIRCUIT_SIZE="14..20"
  export KECCAK_SPONGE_CIRCUIT_SIZE="9..15"
  export LOGIC_CIRCUIT_SIZE="12..18"
  export MEMORY_CIRCUIT_SIZE="17..28"
  export MEM_BEFORE_CIRCUIT_SIZE="8..23"
  export MEM_AFTER_CIRCUIT_SIZE="8..27"
fi

PROOF_OUTPUT_DIR="proofs"
OUT_LOG_PATH="${PROOF_OUTPUT_DIR}/b${i}.log"
ALWAYS_WRITE_LOGS=0 # Change this to `1` if you always want logs to be written.
TOT_BLOCKS=$(($2-$1+1))

START_BLOCK=$1
END_BLOCK=$2
NODE_RPC_URL=$3
IGNORE_PREVIOUS_PROOFS=$4


mkdir -p $PROOF_OUTPUT_DIR


if [ $IGNORE_PREVIOUS_PROOFS ]; then
    # Set checkpoint height to previous block number for the first block in range
    prev_proof_num=$(($1-1))
    PREV_PROOF_EXTRA_ARG="--checkpoint-block-number ${prev_proof_num}"
else
    if [ $1 -gt 1 ]; then
        prev_proof_num=$(($1-1))
        PREV_PROOF_EXTRA_ARG="-f ${PROOF_OUTPUT_DIR}/b${prev_proof_num}.zkproof"
    fi
fi

# Convert hex to decimal parameters
if [[ $START_BLOCK == 0x* ]]; then
    START_BLOCK=$((16#${START_BLOCK#"0x"}))
fi
if [[ $END_BLOCK == 0x* ]]; then
    END_BLOCK=$((16#${END_BLOCK#"0x"}))
fi

# Define block interval
if [ $START_BLOCK == $END_BLOCK ]; then
      BLOCK_INTERVAL=$((16#${START_BLOCK#"0x"}))
else
    BLOCK_INTERVAL=$START_BLOCK..=$END_BLOCK
fi


# If we set test_only flag, we'll generate a dummy
# proof. This is useful for quickly testing decoding and all of the
# other non-proving code.
if [[ $5 == "test_only" ]]; then
    # test only run
    echo "Proving blocks ${BLOCK_INTERVAL} in a test_only mode now... (Total: ${TOT_BLOCKS})"
    cargo r --release --features test_only --bin leader -- --runtime in-memory --load-strategy on-demand jerigon --rpc-url "$NODE_RPC_URL" --block-interval $BLOCK_INTERVAL --proof-output-dir $PROOF_OUTPUT_DIR $PREV_PROOF_EXTRA_ARG > $OUT_LOG_PATH 2>&1
    if grep 'Successfully generated witness for block' $OUT_LOG_PATH; then
        echo "Success - Note this was just a test, not a proof"
        # Remove the log on success if we don't want to keep it.
        if [ $ALWAYS_WRITE_LOGS -ne 1 ]; then
            rm $OUT_LOG_PATH
        fi
        exit
    else
        echo "Failed to create a witness"
        exit 1
    fi
else
    # normal run
    echo "Proving blocks ${BLOCK_INTERVAL} now... (Total: ${TOT_BLOCKS})"
    cargo r --release --bin leader -- --runtime in-memory --load-strategy on-demand jerigon --rpc-url "$3" --block-interval $BLOCK_INTERVAL --proof-output-dir $PROOF_OUTPUT_DIR $PREV_PROOF_EXTRA_ARG > $OUT_LOG_PATH 2>&1

    retVal=$?
    if [ $retVal -ne 0 ]; then
        # Some error occured.
        echo "Block ${i} errored. See ${OUT_LOG_PATH} for more details."
        exit $retVal
    else
        # Remove the log on success if we don't want to keep it.
        if [ $ALWAYS_WRITE_LOGS -ne 1 ]; then
            rm $OUT_LOG_PATH
        fi
    fi

    echo "Successfully generated ${TOT_BLOCKS} proofs!"
fi




