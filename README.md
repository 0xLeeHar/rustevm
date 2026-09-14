# rustevm

A Rust-to-EVM compiler: `no_std` Rust compiled straight to EVM bytecode through a
custom rustc codegen backend — no WASM or Solidity in the middle — plus a
`std`-like runtime layer (`std-evm`) for actually writing contracts.

```text
contract crate (no_std Rust, uses std-evm)
    │  rustc frontend  →  MIR
    ▼
rustc_codegen_evm        (rustc shim, loaded as a dylib via -Zcodegen-backend)
    │  MIR → EIR
    ▼
evm-codegen              (IR → opt → stackify → assemble → link)
    │
    ▼
EVM bytecode             (checked against revm by evm-testkit)
```

Design notes live in [`design/`](design/) — start with
[`rust-evm-compiler-design.md`](design/rust-evm-compiler-design.md); the crate
inventory there (§18) is the source of truth for the layout below.

## Layout

```
src/
  shared/   evm-isa                   opcode table, read by both sides
  backend/  evm-codegen               the backend pipeline
            rustc_codegen_evm         the rustc shim
  target/   evm-sys, evm-sys-macros   raw opcode bindings
            std-evm-abi               ABI logic shared by macro and runtime
            std-evm-macros, std-evm   the user-facing layer
  tooling/  evm-testkit               run bytecode on revm and assert
examples/   my-first-contract         a contract written against std-evm
```

The split is by *which machine the code runs on*: `backend/` and `tooling/` run
on the host at compile time, `target/` runs inside the EVM, and `shared/` is
the one crate both sides read.

## The crates

**`evm-isa`** — the canonical instruction set as pure data: byte, arity, gas
class, category, minimum fork, plus the `Fork` ordering and the PUSH/DUP/SWAP
families. Zero dependencies and stable-only, because it is read by a
nightly-only compiler backend *and* a stable-only proc-macro crate. Nothing
else in the workspace restates an opcode byte or an arity.

**`evm-codegen`** — the whole backend pipeline as one crate of internal
modules. `asm` (assembler + disassembler) and `ir` (EIR: typed, SSA,
block-structured, hand-writable) exist today; stackify, opt and link are the
remaining pieces.

**`rustc_codegen_evm`** — the rustc shim, built as a dylib rustc `dlopen`s. The
only crate pinned to nightly (`rustc_private`). Currently a working
`CodegenBackend` skeleton — the MIR→EIR lowering itself is the open work.

**`evm-sys`** — raw `unsafe` bindings, one per opcode, grouped by category
(arithmetic, storage, call, …). The bodies are unreachable markers; the backend
recognises them by link-name and emits the opcode directly.

**`evm-sys-macros`** — the `#[evm_opcode]` attribute that stamps those
bindings, cross-checking each signature against `evm-isa`'s arity at compile
time.

**`std-evm-abi`** — pure ABI logic: Rust-type→ABI-type mapping, selector
computation, head/tail encoding, `U256`/`Word`, revert encoding. It exists to
break the cycle between the macros and the runtime, which both need it.

**`std-evm-macros`** — `#[contract]`, `#[constructor]`, `#[storage]`,
`#[transient]`, and the ABI derives. Deliberately thin: the tricky logic lives
in `std-evm-abi` where it can be unit-tested.

**`std-evm`** — the safe layer and the only crate a contract author depends on.
Traits do the work (`Method`/`Dispatch`/`Contract`), typed storage handles
(`Storage`, `TransientStorage`, `Mapping`, guards) sit over the raw opcodes,
and the macros above fill the two reflection gaps Rust can't: enumerating all
methods and all fields.

**`evm-testkit`** — the differential harness: assemble, run on revm, assert on
what happened. A failed assertion prints the full instruction trace, since revm
execution is the debugger for everything built above the assembler.

**`my-first-contract`** — an example contract exercising storage, transient
storage, mappings, a constructor and a payable method.

## Getting started

### Prerequisites

- [`rustup`](https://rustup.rs) with both a stable and a nightly toolchain
- [`just`](https://github.com/casey/just) — every workflow below is a recipe
- `rustc-dev` and `rust-src` on nightly, so `rustc_codegen_evm` can link the
  compiler internals it uses via `rustc_private` and `build-std` can rebuild
  `core`/`alloc` for the EVM target

```sh
just setup     # installs nightly + rustc-dev, llvm-tools, rust-src
just test      # should be green before you change anything
```

### The toolchain split

`rustc_codegen_evm` is the *only* nightly-pinned crate, and keeping it that way
is deliberate — `evm-isa` in particular has to build on stable because a
stable-only proc-macro crate reads it. That is why the recipes look doubled up:
`just test` and `just check` run the workspace on stable with that one crate
excluded, then run it alone on nightly, and `just check` additionally builds
`evm-isa` on stable on its own to catch a nightly-only feature creeping in.

### Everyday recipes

| Recipe | What it does |
|---|---|
| `just test` | Workspace tests, both toolchains |
| `just check` | Clippy (`-D warnings`) + `cargo fmt --check`, both toolchains |
| `just fmt` | Format everything (`rustfmt.toml`: 120 columns) |
| `just build` | Backend, target crates and examples |
| `just build-contract [crate]` | Compile an example *with the EVM backend* — `-Zcodegen-backend` pointed at the built dylib, `-Z build-std=core,alloc`, against `evm-unknown-none.json` |
| `just dump-mir [crate]` | Dump MIR to `.mir-dumps/` — the input the backend has to lower |
| `just trace [crate]` | `build-contract` with `RUSTC_LOG=rustc_codegen_evm=debug` |
| `just clean` | `cargo clean` plus the MIR dumps |

### Where to start reading

Follow the pipeline bottom-up — that is also the order the project was built in,
and each layer is the oracle for the one above it:

1. **`evm-isa`** (`src/shared/evm-isa`) — small, pure data, no dependencies.
   Everything else defers to it for opcode bytes, arities and fork gating.
2. **`evm-codegen::asm`** (`src/backend/evm-codegen/src/asm.rs`) — emit opcodes
   and labels, get bytes; `disasm` reads them back.
3. **`evm-testkit`** (`src/tooling/evm-testkit`) — assemble, run on revm, assert.
   The crate docs open with a runnable example, and a failed assertion prints
   the full instruction trace.
4. **`evm-codegen::ir`** (`src/backend/evm-codegen/src/ir.rs`) — EIR, typed and
   SSA, written by hand through `ir::builder`. Read the module docs first: the
   `Repair` field and block-parameter choices are the load-bearing decisions.
5. **`std-evm`** and `examples/my-first-contract` — the other end of the
   project, and independent of the backend's progress.

### Conventions

- Tests live in a `tests.rs` beside the module they cover (`asm/tests.rs`,
  `ir/builder/tests.rs`, …), wired up with `#[cfg(test)] mod tests;`.
- Nothing outside `evm-isa` restates an opcode byte, an arity or a fork gate —
  if you need one, look it up through that crate.
- New backend work should be reachable from hand-written EIR and covered by an
  `evm-testkit` run on revm, so that when the rustc shim lands you can bisect a
  miscompile to above or below the IR.

### Working on the rustc backend

`rustc_codegen_evm` builds as a dylib that rustc `dlopen`s. After
`just build-backend`, `just build-contract` compiles the example through it —
today that produces an empty module, since MIR→EIR lowering is still open
(`src/backend/rustc_codegen_evm/src/backend.rs`). Because it links compiler
internals, it will break on a nightly bump; that is expected, and the fix is
normally a signature change in `backend.rs`.
