#!/usr/bin/env bash
# ============================================================================
# scripts/run_all_benchmarks.sh
# Master Automated Systems Benchmark & Verification Runner for Y Code
# Exits non-zero on the first failure; every gate below is enforced.
# ============================================================================

set -euo pipefail

echo "=========================================================================="
echo "          STARTING Y CODE MASTER SYSTEMS VERIFICATION              "
echo "=========================================================================="

echo "[1/5] Checking Formatting..."
cargo fmt --all -- --check

echo "[2/5] Running Clippy (deny warnings)..."
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo "[3/5] Executing Multi-Crate Test Matrix..."
cargo test --workspace --all-features --locked

echo "[4/5] Building Fully Optimized Release Binaries (LTO = fat)..."
cargo build --release --workspace --locked

echo "[5/5] Executing Systems Verification & Guardrails Benchmark..."
# Must be a release build: the guardrails are performance budgets and a debug
# build does not measure what ships. The binary exits non-zero on any breach.
./target/release/y-bench

echo "=========================================================================="
echo "          Y CODE VERIFICATION COMPLETE: ALL GATES PASSED             "
echo "=========================================================================="