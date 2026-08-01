//! What a transaction did: how it ended, what it returned, what it cost.
//!
//! The accessors that name a type (`returned_u64` and friends) panic rather
//! than return a `Result`, and panic with the whole [`Outcome`] — status,
//! revert reason, and trace. A test asserting `5` should not have to unwrap
//! its way to the failure, and "returned 4 not 5" is far less useful than the
//! instruction that produced the 4.

use std::fmt;

use revm::context_interface::result::{ExecutionResult, HaltReason, Output};
use revm::primitives::{Address, Log, U256};

use crate::trace::Trace;

/// How execution ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// `STOP`, `RETURN`, or running off the end of the code.
    Success,
    /// `REVERT` — state rolled back, unspent gas returned.
    Revert,
    /// Aborted with no refund: bad jump, stack over/underflow, out of gas,
    /// an opcode that does not exist on this fork.
    Halt(HaltReason),
}

/// The result of one transaction.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub status: Status,
    /// `RETURN`/`REVERT` data, or empty.
    pub output: Vec<u8>,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    /// The address a `CREATE` produced, if this was a deployment.
    pub created: Option<Address>,
    /// Present when the [`Testkit`](crate::Testkit) was built with tracing on.
    pub trace: Option<Trace>,
}

impl Outcome {
    pub(crate) fn new(result: ExecutionResult, trace: Option<Trace>) -> Self {
        match result {
            ExecutionResult::Success { gas, logs, output, .. } => {
                let (data, created) = match output {
                    Output::Call(data) => (data, None),
                    Output::Create(data, address) => (data, address),
                };
                Outcome {
                    status: Status::Success,
                    output: data.to_vec(),
                    gas_used: gas.tx_gas_used(),
                    logs,
                    created,
                    trace,
                }
            }
            ExecutionResult::Revert { gas, logs, output } => Outcome {
                status: Status::Revert,
                output: output.to_vec(),
                gas_used: gas.tx_gas_used(),
                logs,
                created: None,
                trace,
            },
            ExecutionResult::Halt { reason, gas, logs } => Outcome {
                status: Status::Halt(reason),
                // A halt discards the frame's memory; there is nothing to return.
                output: Vec::new(),
                gas_used: gas.tx_gas_used(),
                logs,
                created: None,
                trace,
            },
        }
    }

    pub fn is_success(&self) -> bool {
        self.status == Status::Success
    }

    pub fn is_revert(&self) -> bool {
        self.status == Status::Revert
    }

    pub fn is_halt(&self) -> bool {
        matches!(self.status, Status::Halt(_))
    }

    /// The returned bytes, panicking with the full outcome if the call did not
    /// succeed.
    pub fn returned(&self) -> &[u8] {
        if !self.is_success() {
            panic!("expected success:\n{self}");
        }
        &self.output
    }

    /// The returned bytes as one 32-byte word.
    pub fn returned_word(&self) -> [u8; 32] {
        let out = self.returned();
        let Ok(word) = <[u8; 32]>::try_from(out) else {
            panic!("expected a 32-byte word, got {} bytes:\n{self}", out.len());
        };
        word
    }

    /// The returned word as a `U256`.
    pub fn returned_u256(&self) -> U256 {
        U256::from_be_bytes(self.returned_word())
    }

    /// The returned word as a `u64`.
    ///
    /// Panics if the upper 192 bits are set — a value that does not fit is
    /// nearly always a wrong mask or a wrong offset, and silently truncating
    /// it hides exactly the bug this crate exists to catch.
    pub fn returned_u64(&self) -> u64 {
        let word = self.returned_word();
        if word[..24].iter().any(|&b| b != 0) {
            panic!("returned word does not fit in u64:\n{self}");
        }
        u64::from_be_bytes(word[24..].try_into().expect("8 bytes"))
    }

    /// The address a deployment created, panicking with the full outcome if
    /// there was none.
    pub fn created_address(&self) -> Address {
        match self.created {
            Some(address) => address,
            None => panic!("expected a deployment:\n{self}"),
        }
    }

    /// The revert message, if the output is a Solidity-style `Error(string)`.
    ///
    /// Returns `None` for a bare `REVERT` and for any other encoding — the
    /// raw bytes are still on [`output`](Self::output).
    pub fn revert_reason(&self) -> Option<String> {
        const ERROR_STRING: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];

        let body = self.output.strip_prefix(&ERROR_STRING)?;
        // head: 32-byte offset to the tail, then at the tail a 32-byte length
        // followed by the bytes themselves.
        let offset = usize::try_from(U256::from_be_slice(body.get(..32)?)).ok()?;
        let len_at = body.get(offset..offset + 32)?;
        let len = usize::try_from(U256::from_be_slice(len_at)).ok()?;
        let bytes = body.get(offset + 32..offset + 32 + len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.status {
            Status::Success => write!(f, "success")?,
            Status::Revert => match self.revert_reason() {
                Some(reason) => write!(f, "revert: {reason}")?,
                None => write!(f, "revert")?,
            },
            Status::Halt(reason) => write!(f, "halt: {reason:?}")?,
        }
        write!(f, " (gas {})", self.gas_used)?;

        if !self.output.is_empty() {
            write!(f, "\noutput: 0x")?;
            for byte in &self.output {
                write!(f, "{byte:02x}")?;
            }
        }
        if let Some(address) = self.created {
            write!(f, "\ncreated: {address}")?;
        }
        match &self.trace {
            Some(trace) => write!(f, "\n{trace}"),
            // Worth saying: the trace is the thing you want here, and its
            // absence is a setting, not a limitation.
            None => write!(f, "\n(no trace — build the Testkit with `.tracing()`)"),
        }
    }
}
