target_json := justfile_directory() / "evm-unknown-none.json"
lib_ext := if os() == "macos" { ".dylib" } else { ".so" }
backend := justfile_directory() / "target/debug/librustc_codegen_evm" + lib_ext

default:
    @just --list

# Test the workspace
test:
    # rustc_codegen_evm links rustc's internals via `rustc_private`, so it only
    # builds on nightly. The rest of the workspace is stable, and stays that way.
    cargo test --workspace --exclude rustc_codegen_evm
    cargo +nightly test -p rustc_codegen_evm

# General clippy and fmt checks
check:
    # Same split as `test`: rustc_codegen_evm is the only nightly-pinned crate.
    cargo clippy --workspace --all-targets --exclude rustc_codegen_evm -- -D warnings
    cargo +nightly clippy -p rustc_codegen_evm --all-targets -- -D warnings
    cargo fmt --check
    # evm-isa is read by both a nightly-only backend and a stable-only proc-macro
    # crate; guard against a nightly-only feature creeping into its stable half.
    cargo +stable check -p evm-isa

# Cargo fmt globally
fmt:
    cargo fmt

# ---------------------------------------------------------------
# building
# ---------------------------------------------------------------

# Build the backend codegen and the target-side crates
build: build-backend build-std build-examples

# Build the backend codegen
build-backend: build-shared
    cargo +nightly build -p rustc_codegen_evm

# Compile a contract with the EVM backend instead of LLVM
build-contract crate="my-first-contract": build-backend
    RUSTFLAGS="-Zcodegen-backend={{backend}}" \
        cargo +nightly build --manifest-path examples/{{crate}}/Cargo.toml \
            -Z build-std=core,alloc \
            -Zjson-target-spec \
            --target {{target_json}}

# Build the src/target/ crates on the host with stable rustc (no backend, no custom target spec).
build-std: build-shared
    cargo build -p evm-sys -p evm-sys-macros -p std-evm -p std-evm-macros -p std-evm-abi

# Build the shared evm-isa crate on both nightly and stable toolchains
build-shared:
    cargo +stable build -p evm-isa
    cargo +nightly build -p evm-isa

build-examples:
    cargo build -p my-first-contract


# ---------------------------------------------------------------
# debugging
# ---------------------------------------------------------------

dump-mir crate="my-first-contract":
    cargo +nightly rustc -p {{ crate }} -- \
        -Zdump-mir=all -Zdump-mir-dir={{ justfile_directory() }}/.mir-dumps

trace crate="my-first-contract":
    RUSTC_LOG=rustc_codegen_evm=debug RUST_BACKTRACE=1 \
        just build-contract {{ crate }}

# ---------------------------------------------------------------
# setup
# ---------------------------------------------------------------

# Install the needed compiler libs and toolchains
setup:
    rustup toolchain install nightly
    rustup component add rustc-dev llvm-tools rust-src --toolchain nightly

# Clean all of the builds and clear the mir bumps
clean: clean-dumps
    cargo clean

# RM the mir dumps
clean-dumps:
    rm -rf {{ justfile_directory() }}/.mir-dumps
