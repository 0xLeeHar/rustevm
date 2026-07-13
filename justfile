# The compiled backend rustc will dlopen. Cargo names it .dylib on macOS, .so elsewhere.
backend := justfile_directory() / "target/debug/librustc_codegen_evm" + if os() == "macos" { ".dylib" } else { ".so" }

# Build everything
build:
    cargo b

# Build the codegen on nightly
build-codegen:
    cargo +nightly build -p rustc_codegen_evm

# Compile the example contract with the EVM backend instead of LLVM.
build-contract: build-codegen
    RUSTFLAGS="-Zcodegen-backend={{ backend }}" cargo +nightly build -p my-first-contract
