#!/usr/bin/env bash
# ================================================================
# DEFINITE SOLVER — QuickPod Deployment Script
# ================================================================
# Deploys the solver on a QuickPod GPU instance (RTX 4090 / A100).
#
# Usage:
#   bash deploy.sh                    # Full setup + build + run P71
#   bash deploy.sh --build-only       # Just build, don't run
#   bash deploy.sh --target 135       # Target P135 instead
#   bash deploy.sh --weight 15        # Search up to weight 15
#
# QuickPod requirements:
#   - GPU: RTX 4090 (24GB) or A100 (40GB+)
#   - OS: Ubuntu 22.04
#   - Disk: 50GB+
#   - CUDA: 12.0+ pre-installed
# ================================================================

set -euo pipefail

# Config
TARGET=${DEFINITE_TARGET:-71}
MAX_WEIGHT=${DEFINITE_WEIGHT:-10}
BUILD_ONLY=false
GPU_SM=89  # RTX 4090; use 80 for A100, 90 for H100

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --target) TARGET="$2"; shift 2 ;;
        --weight) MAX_WEIGHT="$2"; shift 2 ;;
        --build-only) BUILD_ONLY=true; shift ;;
        --sm) GPU_SM="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: bash deploy.sh [--target N] [--weight W] [--build-only] [--sm N]"
            echo "  --target N    Puzzle number (default: 71)"
            echo "  --weight W    Max Hamming weight (default: 10)"
            echo "  --build-only  Build only, don't run"
            echo "  --sm N        GPU compute capability (default: 89 for RTX 4090)"
            exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "╔══════════════════════════════════════════════════════════╗"
echo "║  DEFINITE SOLVER — GPU Deployment                       ║"
echo "║  Target: P${TARGET}  |  Weight: ≤${MAX_WEIGHT}  |  SM: ${GPU_SM}        ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# ============================================================
# Step 1: System Check
# ============================================================
echo "[1/6] System check..."

# Check GPU
if ! command -v nvidia-smi &>/dev/null; then
    echo "  ERROR: nvidia-smi not found. No NVIDIA GPU detected."
    echo "  This script requires a GPU instance on QuickPod."
    exit 1
fi

GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)
GPU_MEM=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader | head -1)
echo "  GPU: ${GPU_NAME} (${GPU_MEM})"

# Check CUDA
if ! command -v nvcc &>/dev/null; then
    echo "  ERROR: nvcc not found. CUDA Toolkit not installed."
    echo "  Install: sudo apt install nvidia-cuda-toolkit"
    exit 1
fi
CUDA_VER=$(nvcc --version | grep release | awk '{print $5}' | sed 's/,//')
echo "  CUDA: ${CUDA_VER}"

# Check Rust
if ! command -v cargo &>/dev/null; then
    echo "  Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
RUST_VER=$(rustc --version | awk '{print $2}')
echo "  Rust: ${RUST_VER}"

echo ""

# ============================================================
# Step 2: Install Dependencies
# ============================================================
echo "[2/6] Installing dependencies..."

sudo apt-get update -qq
sudo apt-get install -y -qq build-essential pkg-config libssl-dev git

echo "  Done."
echo ""

# ============================================================
# Step 3: Clone / Verify Repo
# ============================================================
echo "[3/6] Repository check..."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ ! -f "${SCRIPT_DIR}/vortex_core/Cargo.toml" ]]; then
    echo "  ERROR: Cannot find vortex_core/Cargo.toml"
    echo "  Make sure you're running this from the repo root."
    exit 1
fi

echo "  Repo at: ${SCRIPT_DIR}"
echo ""

# ============================================================
# Step 4: Build CUDA Kernels
# ============================================================
echo "[4/6] Building CUDA kernels..."

cd "${SCRIPT_DIR}/RUSTSOLVER/cuda"

echo "  Compiling sparse.cu → sparse.ptx (SM=${GPU_SM})..."
make ptx SM=${GPU_SM} 2>&1 | tail -5

echo "  PTX files:"
ls -lh *.ptx 2>/dev/null || echo "  WARNING: No PTX files generated"
echo ""

# ============================================================
# Step 5: Build Rust Solver
# ============================================================
echo "[5/6] Building Rust solver (release)..."

cd "${SCRIPT_DIR}/vortex_core"
cargo build --release 2>&1 | tail -5

BINARY="${SCRIPT_DIR}/vortex_core/target/release/vortex-gpu"
if [[ -f "${BINARY}" ]]; then
    echo "  Binary: ${BINARY}"
    echo "  Size: $(du -h ${BINARY} | cut -f1)"
else
    echo "  ERROR: Binary not found at ${BINARY}"
    exit 1
fi
echo ""

# ============================================================
# Step 6: Run the Solver
# ============================================================
if [[ "${BUILD_ONLY}" == true ]]; then
    echo "[6/6] Build-only mode. Skipping run."
    echo ""
    echo "  To run manually:"
    echo "  ${BINARY} --mode sparse-gpu --target ${TARGET} --max-weight ${MAX_WEIGHT}"
    exit 0
fi

echo "[6/6] Launching sparse search on P${TARGET} (weight ≤ ${MAX_WEIGHT})..."
echo ""
echo "  ╔══════════════════════════════════════════════════════╗"
echo "  ║  P${TARGET} — SPARSE KEY SEARCH — GPU MODE           ║"
echo "  ║  Weight ≤ ${MAX_WEIGHT}                                     ║"
echo "  ╚══════════════════════════════════════════════════════╝"
echo ""

cd "${SCRIPT_DIR}"
${BINARY} --mode sparse-gpu --target ${TARGET} --max-weight ${MAX_WEIGHT}

echo ""
echo "  NOUS SOMMES LES RECHERCHES."
