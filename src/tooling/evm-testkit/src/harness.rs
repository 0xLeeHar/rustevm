//! The harness: bytecode in, [`Outcome`] out.
//!
//! State persists across calls on one [`Testkit`], so a test can deploy, call,
//! and then read the storage the call wrote. Each call is its own transaction
//! against the same in-memory database.

use std::mem;

use evm_codegen::Asm;
use evm_isa::Fork;
use revm::bytecode::Bytecode;
use revm::context::TxEnv;
use revm::database::{CacheDB, EmptyDB, InMemoryDB};
use revm::inspector::InspectCommitEvm;
use revm::primitives::{Address, Bytes, TxKind, U256, address};
use revm::state::AccountInfo;
use revm::{Context, Database, ExecuteCommitEvm, MainBuilder, MainContext};

use crate::fork::spec_id;
use crate::link::deployer;
use crate::outcome::Outcome;
use crate::trace::Tracer;

/// Enough gas that nothing a unit test writes runs out by accident, and few
/// enough that a runaway loop terminates before the test suite does.
const DEFAULT_GAS_LIMIT: u64 = 30_000_000;

/// A throwaway chain holding one contract under test.
pub struct Testkit {
    fork: Fork,
    db: InMemoryDB,
    caller: Address,
    gas_limit: u64,
    tracing: bool,
}

impl Testkit {
    /// Where [`run`](Self::run) plants the code under test.
    pub const CONTRACT: Address = address!("00000000000000000000000000000000000c0de0");

    /// The default `CALLER`/`ORIGIN`.
    pub const CALLER: Address = address!("000000000000000000000000000000000000ca11");

    pub fn new(fork: Fork) -> Self {
        let mut tk = Testkit {
            fork,
            db: CacheDB::new(EmptyDB::default()),
            caller: Self::CALLER,
            gas_limit: DEFAULT_GAS_LIMIT,
            tracing: false,
        };
        // Transactions here are sent at a zero gas price, so this is not
        // needed to pay for gas — it is here so a test can send `value`
        // without first remembering to fund anybody.
        tk.set_balance(Self::CALLER, U256::MAX);
        tk
    }

    /// Record an instruction trace on every [`Outcome`].
    ///
    /// Off by default: it allocates a stack snapshot per instruction, which is
    /// wasted work for a test that passes. Turn it on when one doesn't.
    pub fn tracing(mut self) -> Self {
        self.tracing = true;
        self
    }

    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn with_caller(mut self, caller: Address) -> Self {
        self.caller = caller;
        self
    }

    // --- running code ---

    /// Plant `code` at [`CONTRACT`](Self::CONTRACT) as already-deployed runtime
    /// code and call it with empty calldata.
    ///
    /// No `CREATE`, no constructor: the point is to execute a fragment exactly
    /// as assembled, without a deployer's bytes in the way.
    pub fn run(&mut self, code: &[u8]) -> Outcome {
        self.run_with(code, &[])
    }

    /// [`run`](Self::run), with calldata.
    pub fn run_with(&mut self, code: &[u8], calldata: &[u8]) -> Outcome {
        self.set_code(Self::CONTRACT, code);
        self.call(Self::CONTRACT, calldata)
    }

    /// Assemble and [`run`](Self::run).
    ///
    /// Panics on an assembler error — in a test, code that does not assemble
    /// is a failure to report, not a case to handle.
    pub fn run_asm(&mut self, asm: Asm) -> Outcome {
        let code = asm
            .finish()
            .unwrap_or_else(|e| panic!("assembling the code under test: {e}"));
        self.run(&code)
    }

    /// Call an address already carrying code.
    pub fn call(&mut self, to: Address, calldata: &[u8]) -> Outcome {
        self.transact(
            TxEnv::builder()
                .caller(self.caller)
                .kind(TxKind::Call(to))
                .data(Bytes::copy_from_slice(calldata))
                .gas_limit(self.gas_limit)
                .gas_price(0)
                .build_fill(),
        )
    }

    /// Run `initcode` through `CREATE`; the deployed address is on
    /// [`Outcome::created`].
    pub fn deploy(&mut self, initcode: &[u8]) -> Outcome {
        self.transact(
            TxEnv::builder()
                .caller(self.caller)
                .kind(TxKind::Create)
                .data(Bytes::copy_from_slice(initcode))
                .gas_limit(self.gas_limit)
                .gas_price(0)
                .build_fill(),
        )
    }

    /// Wrap `runtime` in a [`deployer`] prologue and [`deploy`](Self::deploy) it.
    pub fn deploy_runtime(&mut self, runtime: &[u8]) -> Outcome {
        let initcode = deployer(runtime, self.fork).unwrap_or_else(|e| panic!("assembling the deployer prologue: {e}"));
        self.deploy(&initcode)
    }

    // --- inspecting and seeding state ---

    pub fn storage(&mut self, at: Address, slot: U256) -> U256 {
        self.db.storage(at, slot).expect("in-memory db cannot fail")
    }

    pub fn balance(&mut self, of: Address) -> U256 {
        self.account(of).balance
    }

    pub fn code(&mut self, at: Address) -> Vec<u8> {
        self.account(at)
            .code
            .map(|code| code.original_bytes().to_vec())
            .unwrap_or_default()
    }

    pub fn set_storage(&mut self, at: Address, slot: U256, value: U256) {
        self.db
            .insert_account_storage(at, slot, value)
            .expect("in-memory db cannot fail");
    }

    pub fn set_balance(&mut self, of: Address, balance: U256) {
        self.update_account(of, |info| info.balance = balance);
    }

    pub fn set_code(&mut self, at: Address, code: &[u8]) {
        let bytecode = Bytecode::new_legacy(Bytes::copy_from_slice(code));
        self.update_account(at, |info| {
            // `insert_account_info` recomputes the hash from the code, but
            // only when it still reads as empty.
            info.code_hash = bytecode.hash_slow();
            info.code = Some(bytecode.clone());
        });
    }

    // --- internals ---

    fn account(&mut self, at: Address) -> AccountInfo {
        self.db.basic(at).expect("in-memory db cannot fail").unwrap_or_default()
    }

    /// Read-modify-write, so setting one field does not clear the others.
    fn update_account(&mut self, at: Address, f: impl FnOnce(&mut AccountInfo)) {
        let mut info = self.account(at);
        f(&mut info);
        self.db.insert_account_info(at, info);
    }

    fn transact(&mut self, tx: TxEnv) -> Outcome {
        // revm's builder consumes the database, so lend it out and take it
        // back once the transaction has been committed to it.
        let db = mem::replace(&mut self.db, CacheDB::new(EmptyDB::default()));
        let fork = self.fork;
        let ctx = Context::mainnet().with_db(db).modify_cfg_chained(|cfg| {
            cfg.spec = spec_id(fork);
            // Every call is built from scratch with a nonce of zero; a testkit
            // is not modelling a mempool.
            cfg.disable_nonce_check = true;
        });

        let (result, trace) = if self.tracing {
            let mut evm = ctx.build_mainnet_with_inspector(Tracer::default());
            let result = evm.inspect_tx_commit(tx);
            let trace = mem::take(&mut evm.inspector).finish();
            self.db = evm.ctx.journaled_state.database;
            (result, Some(trace))
        } else {
            let mut evm = ctx.build_mainnet();
            let result = evm.transact_commit(tx);
            self.db = evm.ctx.journaled_state.database;
            (result, None)
        };

        // This is validation failing before a single opcode ran — a bad nonce,
        // a gas limit under the intrinsic cost. Not a result the code under
        // test produced, so it is not something to assert against.
        let result = result.unwrap_or_else(|e| panic!("transaction rejected before execution: {e}"));
        Outcome::new(result, trace)
    }
}
