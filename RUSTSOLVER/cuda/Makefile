# VORTEX GPU — CUDA Build Configuration
# ==================================================
# Compile the CUDA kangaroo kernel to PTX for cudarc loading.
#
# Requirements:
#   - CUDA Toolkit 11.0+ (nvcc compiler)
#   - NVIDIA GPU with compute capability 7.0+ (Volta or newer)
#
# Build:
#   make              # Build PTX for RTX 4090 (CC 8.9)
#   make SM=80        # Build for A100
#   make SM=90        # Build for H100
#   make all          # Build PTX + standalone binary
#
# The PTX file is loaded at runtime by the Rust GPU bridge (cudarc).

NVCC = nvcc
NVCC_FLAGS = -O3 -use_fast_math -Xcompiler -fPIC

# Default: RTX 4090 (compute capability 8.9)
SM ?= 89
ARCH_FLAG = -arch=sm_$(SM)

# Source files
SOURCES = kangaroo.cu
HEADERS = secp256k1.cuh

# Output
PTX_TARGET = kangaroo.ptx
BIN_TARGET = vortex_cuda

# Debug flags
ifeq ($(DEBUG), 1)
    NVCC_FLAGS += -G -g -lineinfo
endif

# Verbose (show register usage)
ifeq ($(VERBOSE), 1)
    NVCC_FLAGS += -Xptxas -v
endif

# Default: build PTX only (for cudarc runtime loading)
ptx: $(PTX_TARGET)

# Build standalone binary (for testing without Rust)
bin: $(BIN_TARGET)

all: ptx bin

$(PTX_TARGET): $(SOURCES) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -ptx -o $@ $(SOURCES)

$(BIN_TARGET): $(SOURCES) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -o $@ $(SOURCES) -lcudart

clean:
	rm -f $(PTX_TARGET) $(BIN_TARGET)

# Print GPU info
info:
	@nvidia-smi || echo "No NVIDIA GPU detected"

# Quick test on small range
test: $(BIN_TARGET)
	./$(BIN_TARGET)

.PHONY: ptx bin all clean info test
