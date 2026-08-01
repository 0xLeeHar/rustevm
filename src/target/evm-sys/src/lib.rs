#![no_std]
#![allow(unused)]
// `DUP8`..`DUP16` and `SWAP7`..`SWAP16` take eight to seventeen operands
// because that is the opcode's stack arity, which `evm-isa` dictates and this
// crate only mirrors. There is no shorter signature to refactor towards.
#![allow(clippy::too_many_arguments)]
// Every binding here is unsafe for the same structural reason — it is not a
// callable function, it is a marker the backend replaces with a raw opcode —
// so `#[evm_opcode]` should stamp the `# Safety` section once it is more than
// a pass-through. Until then, 46 copies of one paragraph would be noise.
#![allow(clippy::missing_safety_doc)]

mod opcode;

pub use opcode::*;
pub use std_evm_abi::{U256, Word};
