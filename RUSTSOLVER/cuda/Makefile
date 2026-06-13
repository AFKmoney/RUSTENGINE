# PRISM VORTEX v13 NEXUS — CUDA Build Configuration
# ==================================================
# Build the CUDA kangaroo kernel for GPU acceleration.
#
# Requirements:
#   - CUDA Toolkit 11.0+ (nvcc compiler)
#   - NVIDIA GPU with compute capability 7.0+ (Volta or newer)
#   - 8+ GB VRAM per GPU for baby step table
#
# Build:
#   make              # Build with default settings (CC 8.9 for RTX 4090)
#   make SM=70        # Build for V100
#   make SM=80        # Build for A100
#   make SM=90        # Build for H100
#   make debug        # Build with debug info
#
# Run:
#   ./nexus_cuda      # Standalone CUDA test
#
# The compiled binary can also be called from Rust via FFI.

NVCC = nvcc
NVCC_FLAGS = -O3 -use_fast_math -Xcompiler -fPIC

# Default: RTX 4090 (compute capability 8.9)
SM ?= 89
ARCH_FLAG = -arch=sm_$(SM)

# Source files
SOURCES = kangaroo.cu
HEADERS = secp256k1.cuh

# Output
TARGET = nexus_cuda

# Debug flags
ifeq ($(DEBUG), 1)
    NVCC_FLAGS += -G -g -lineinfo
endif

# Verbose
ifeq ($(VERBOSE), 1)
    NVCC_FLAGS += -Xptxas -v
endif

all: $(TARGET)

$(TARGET): $(SOURCES) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -o $@ $(SOURCES) -lcudart

clean:
	rm -f $(TARGET)

# Print GPU info
info:
	@nvidia-smi || echo "No NVIDIA GPU detected"

# Quick test on small range
test: $(TARGET)
	./$(TARGET)

.PHONY: all clean info test
