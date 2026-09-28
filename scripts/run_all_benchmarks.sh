#!/usr/bin/env bash
# ============================================================================
# scripts/run_all_benchmarks.sh
# Master Automated Systems Benchmark & Verification Runner for Y Code
# ============================================================================

set -euo pipefail

echo "=========================================================================="
echo "          STARTING Y CODE MASTER SYSTEMS VERIFICATION              "
echo "=========================================================================="

echo "[1/4] Running Cargo Workspace Linter & Format Checks..."
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features

echo "[2/4] Executing Comprehensive Multi-Crate Test Matrix..."
cargo test --workspace --all-features -- --nocapture

echo "[3/4] Building Fully Optimized Release Binaries (LTO = fat)..."
cargo build --release --workspace

echo "[4/4] Executing Master Systems Verification & Guardrails Benchmark..."
./target/release/y-bench

echo "=========================================================================="
echo "          Y CODE VERIFICATION COMPLETE: ALL GATES PASSED (100%)     "
echo "=========================================================================="
