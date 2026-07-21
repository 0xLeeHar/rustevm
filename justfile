# ---- config ----

# rust-toolchain.toml already pins the workspace to nightly (with rustc-dev,
# rust-src, llvm-tools), so bare `cargo` is nightly here. `+nightly` is left
# explicit on the backend recipes to document the hard requirement.

target_json := justfile_directory() / "evm-unknown-none.json"

# The compiled backend rustc will dlopen. Cargo names it .dylib on macOS, .so elsewhere.
# Workspace members share the ROOT target dir — the backend is not under its own crate dir.
lib_ext := if os() == "macos" { ".dylib" } else { ".so" }
backend := justfile_directory() / "target/debug/librustc_codegen_evm" + lib_ext

default:
    @just --list

# ---------------------------------------------------------------
# host half — fast, runs constantly
# ---------------------------------------------------------------

# Everything builds on the host today, including the target libs: evm-sys and
# std-evm are no_std but host-compilable until the backend can emit for them.

# Build the whole workspace on the host
build:
    cargo build --workspace

test:
    cargo test --workspace

check:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --check

fmt:
    cargo fmt

# ---------------------------------------------------------------
# nightly half — the codegen backend
# ---------------------------------------------------------------

build-backend:
    cargo +nightly build -p rustc_codegen_evm

# ---------------------------------------------------------------
# target side — needs the backend + custom target spec + build-std
#
# NOTE: evm-unknown-none.json does not exist yet; design doc §4 has the spec to
# write. Until it lands, these recipes fail fast with a clear message instead of
# a confusing cargo error.
# ---------------------------------------------------------------

_require-target-json:
    #!/usr/bin/env sh
    if [ ! -f "{{ target_json }}" ]; then
        echo "error: {{ target_json }} not found." >&2
        echo "       Write the target spec first — see design/rust-evm-compiler-design.md section 4." >&2
        exit 1
    fi

# Works today (host target, no build-std) — the smoke test for backend loading.

# Compile a contract with the EVM backend instead of LLVM
build-contract crate="my-first-contract": build-backend
    RUSTFLAGS="-Zcodegen-backend={{ backend }}" \
        cargo +nightly build -p {{ crate }}

# Full target build: custom spec + core/alloc compiled from source.
build-target crate="my-first-contract": _require-target-json build-backend
    RUSTFLAGS="-Zcodegen-backend={{ backend }} -Ccodegen-units=1" \
        cargo +nightly build \
            --manifest-path examples/{{ crate }}/Cargo.toml \
            -Zbuild-std=core,alloc \
            -Zbuild-std-features=compiler-builtins-mem \
            --target {{ target_json }} \
            --release

# Design doc section 6: the key integration test.

# Build core/alloc from source through the backend
build-std: _require-target-json build-backend
    RUSTFLAGS="-Zcodegen-backend={{ backend }} -Ccodegen-units=1" \
        cargo +nightly build \
            --manifest-path src/target/std-evm/Cargo.toml \
            -Zbuild-std=core,alloc \
            -Zbuild-std-features=compiler-builtins-mem \
            --target {{ target_json }}

# ---------------------------------------------------------------
# debugging
# ---------------------------------------------------------------

dump-mir crate="my-first-contract":
    cargo +nightly rustc -p {{ crate }} -- \
        -Zdump-mir=all -Zdump-mir-dir={{ justfile_directory() }}/mir-dumps

trace crate="my-first-contract":
    RUSTC_LOG=rustc_codegen_evm=debug RUST_BACKTRACE=1 \
        just build-contract {{ crate }}

# ---------------------------------------------------------------
# setup
# ---------------------------------------------------------------

setup:
    rustup toolchain install nightly
    rustup component add rustc-dev llvm-tools rust-src --toolchain nightly

clean:
    cargo clean
    rm -rf {{ justfile_directory() }}/mir-dumps
