#!/bin/bash
# Build script for PRISM VORTEX v12 CUDA kernels
# Requires: CUDA Toolkit 11.0+, nvcc

set -e

echo "═══ PRISM VORTEX v12 — CUDA Build Script ═══"

# Detect CUDA
if ! command -v nvcc &> /dev/null; then
    echo "ERROR: nvcc not found. Install CUDA Toolkit first."
    echo "  Ubuntu: sudo apt install nvidia-cuda-toolkit"
    echo "  Or download from: https://developer.nvidia.com/cuda-downloads"
    exit 1
fi

NVCC_VERSION=$(nvcc --version | grep "release" | sed 's/.*release //' | sed 's/,.*//')
echo "CUDA version: $NVCC_VERSION"

# Detect GPU architecture
GPU_ARCH=${1:-sm_80}  # Default: A100, change for your GPU
echo "Target architecture: $GPU_ARCH"
echo ""
echo "Common architectures:"
echo "  sm_70  — V100"
echo "  sm_75  — T4, RTX 2080"
echo "  sm_80  — A100, RTX 3090"
echo "  sm_86  — A40, RTX 3080"
echo "  sm_89  — RTX 4090"
echo "  sm_90  — H100"
echo ""

# Build shared library
echo "Compiling CUDA kernels..."
nvcc -arch=$GPU_ARCH -O3 \
     --shared \
     -Xcompiler -fPIC \
     -o target/libprism_cuda.so \
     cuda/prism_cuda.cu

echo "✓ CUDA library built: target/libprism_cuda.so"
echo ""
echo "To run with GPU support:"
echo "  LD_LIBRARY_PATH=target cargo run --release -- --mode prism-gpu --target 135"
