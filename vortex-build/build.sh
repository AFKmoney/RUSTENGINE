#!/bin/bash
# ============================================================
# VORTEX PRIME v4 — Build & Deploy Script
# ============================================================
# Usage:
#   ./build.sh          — Build CPU version
#   ./build.sh cuda     — Build CUDA GPU version
#   ./build.sh run      — Build + Run on CPU
#   ./build.sh cloud    — Build for cloud GPU deployment
# ============================================================

set -e

MODE="${1:-cpu}"
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$PROJECT_DIR/target/release"

echo "╔══════════════════════════════════════════════════════════╗"
echo "║  VORTEX PRIME v4 — Build System                         ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

case $MODE in
  cpu)
    echo "[BUILD] CPU mode (Rayon parallel)"
    cd "$PROJECT_DIR"
    cargo build --release --features cpu
    echo "[OK] Binary: $BUILD_DIR/vortex-gpu"
    ;;

  cuda)
    echo "[BUILD] CUDA GPU mode"
    echo "[BUILD] Compiling CUDA kernel..."
    if command -v nvcc &> /dev/null; then
      mkdir -p "$PROJECT_DIR/target/ptx"
      nvcc -arch=sm_80 -ptx -O3 \
        "$PROJECT_DIR/kernels/vortex_kernel.cu" \
        -o "$PROJECT_DIR/target/ptx/vortex_kernel.ptx"
      echo "[OK] PTX compiled: target/ptx/vortex_kernel.ptx"
    else
      echo "[WARN] nvcc not found. CUDA kernel will not be compiled."
      echo "[WARN] Install CUDA Toolkit: https://developer.nvidia.com/cuda-downloads"
    fi
    cd "$PROJECT_DIR"
    cargo build --release --features cuda
    echo "[OK] Binary: $BUILD_DIR/vortex-gpu"
    ;;

  run)
    echo "[BUILD+RUN] Building and running..."
    cd "$PROJECT_DIR"
    cargo build --release --features cpu
    echo ""
    echo "[RUN] Launching VORTEX PRIME v4..."
    "$BUILD_DIR/vortex-gpu" --mode cpu --target 135 --verbose
    ;;

  cloud)
    echo "[CLOUD] Building for cloud GPU deployment"
    echo ""

    # Check for cloud GPU tools
    if command -v nvidia-smi &> /dev/null; then
      echo "[OK] NVIDIA GPU detected:"
      nvidia-smi --query-gpu=name,memory.total,compute_cap --format=csv,noheader
    else
      echo "[WARN] nvidia-smi not found. No GPU available."
    fi

    # Build CUDA kernel
    if command -v nvcc &> /dev/null; then
      echo ""
      echo "[BUILD] Compiling CUDA kernel for multiple architectures..."
      for ARCH in sm_70 sm_75 sm_80 sm_86 sm_89 sm_90; do
        echo "  Compiling for $ARCH..."
        nvcc -arch=$ARCH -ptx -O3 \
          "$PROJECT_DIR/kernels/vortex_kernel.cu" \
          -o "$PROJECT_DIR/target/ptx/vortex_kernel_${ARCH}.ptx" 2>/dev/null || true
      done
      echo "[OK] PTX kernels compiled"
    fi

    # Build Rust binary
    cd "$PROJECT_DIR"
    cargo build --release --features cuda 2>/dev/null || \
    cargo build --release --features cpu

    echo ""
    echo "[CLOUD] Deployment instructions:"
    echo ""
    echo "  1. Upload to cloud GPU instance:"
    echo "     scp -r $PROJECT_DIR user@gpu-server:/opt/vortex-gpu/"
    echo ""
    echo "  2. On the GPU server, run:"
    echo "     cd /opt/vortex-gpu"
    echo "     ./target/release/vortex-gpu --mode cuda --target 135"
    echo ""
    echo "  3. Recommended cloud GPUs:"
    echo "     - NVIDIA A100 (80GB) — 2x A100 = ~10^10 EC ops/s"
    echo "     - NVIDIA H100 (80GB) — 2x H100 = ~10^11 EC ops/s"
    echo "     - NVIDIA RTX 4090 (24GB) — budget option"
    echo ""
    echo "  4. Cloud providers with GPU:"
    echo "     - AWS: p4d.24xlarge (8x A100) — ~$32/hr"
    echo "     - GCP: a2-ultragpu-8g (8x A100) — ~$30/hr"
    echo "     - Lambda Labs: 2x A100 — ~$4/hr"
    echo "     - RunPod: A100 — ~$1.50/hr"
    echo ""
    echo "  5. Standalone CUDA kernel (no Rust needed):"
    echo "     nvcc -arch=sm_80 -O3 -o vortex_cuda kernels/vortex_kernel.cu"
    echo "     ./vortex_cuda"
    ;;

  kernel)
    echo "[BUILD] Compiling standalone CUDA kernel..."
    if command -v nvcc &> /dev/null; then
      # Detect GPU architecture
      if command -v nvidia-smi &> /dev/null; then
        CAP=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader | head -1 | tr -d '.')
        ARCH="sm_${CAP}"
        echo "  Detected GPU compute capability: $ARCH"
      else
        ARCH="sm_80"
        echo "  Defaulting to $ARCH"
      fi

      nvcc -arch=$ARCH -O3 \
        "$PROJECT_DIR/kernels/vortex_kernel.cu" \
        -o "$BUILD_DIR/vortex_cuda"

      echo "[OK] Standalone CUDA binary: $BUILD_DIR/vortex_cuda"
    else
      echo "[ERROR] nvcc not found. Install CUDA Toolkit first."
      exit 1
    fi
    ;;

  *)
    echo "Usage: $0 {cpu|cuda|run|cloud|kernel}"
    exit 1
    ;;
esac
