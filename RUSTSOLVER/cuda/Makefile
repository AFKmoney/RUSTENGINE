# VORTEX GPU — CUDA Build Configuration
# ==================================================
# Compile the CUDA kernels to PTX for cudarc loading.
#
# Kernels:
#   - kangaroo.cu: Pollard's Kangaroo with GLV √6
#   - bruteforce.cu: Multi-target address checker (SHA256 + RIPEMD160)
#
# Requirements:
#   - CUDA Toolkit 11.0+ (nvcc compiler)
#   - NVIDIA GPU with compute capability 7.0+ (Volta or newer)
#
# Build:
#   make              # Build PTX for RTX 4090 (CC 8.9)
#   make SM=80        # Build for A100
#   make SM=90        # Build for H100
#   make all          # Build all PTX + standalone binaries
#
# The PTX files are loaded at runtime by the Rust GPU bridge (cudarc).

NVCC = nvcc
NVCC_FLAGS = -O3 -use_fast_math -Xcompiler -fPIC

# Default: RTX 4090 (compute capability 8.9)
SM ?= 89
ARCH_FLAG = -arch=sm_$(SM)

# Source files
KANGAROO_SRC = kangaroo.cu
BRUTEFORCE_SRC = bruteforce.cu
HEADERS = secp256k1.cuh

# Output
KANGAROO_PTX = kangaroo.ptx
BRUTEFORCE_PTX = bruteforce.ptx
KANGAROO_BIN = vortex_kangaroo
BRUTEFORCE_BIN = vortex_bruteforce

# Debug flags
ifeq ($(DEBUG), 1)
    NVCC_FLAGS += -G -g -lineinfo
endif

# Verbose (show register usage)
ifeq ($(VERBOSE), 1)
    NVCC_FLAGS += -Xptxas -v
endif

# Default: build all PTX
ptx: $(KANGAROO_PTX) $(BRUTEFORCE_PTX)

# Build individual PTX files
$(KANGAROO_PTX): $(KANGAROO_SRC) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -ptx -o $@ $(KANGAROO_SRC)

$(BRUTEFORCE_PTX): $(BRUTEFORCE_SRC) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -ptx -o $@ $(BRUTEFORCE_SRC)

# Build standalone binaries (for testing without Rust)
bin: $(KANGAROO_BIN) $(BRUTEFORCE_BIN)

$(KANGAROO_BIN): $(KANGAROO_SRC) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -o $@ $(KANGAROO_SRC) -lcudart

$(BRUTEFORCE_BIN): $(BRUTEFORCE_SRC) $(HEADERS)
	$(NVCC) $(NVCC_FLAGS) $(ARCH_FLAG) -o $@ $(BRUTEFORCE_SRC) -lcudart

all: ptx bin

clean:
	rm -f $(KANGAROO_PTX) $(BRUTEFORCE_PTX) $(KANGAROO_BIN) $(BRUTEFORCE_BIN)

# Print GPU info
info:
	@nvidia-smi || echo "No NVIDIA GPU detected"

# Quick test on small range
test: $(KANGAROO_BIN)
	./$(KANGAROO_BIN)

.PHONY: ptx bin all clean info test
