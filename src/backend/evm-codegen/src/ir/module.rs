//! A whole program: functions, their signatures, and which one runs first.
//!
//! Declaration is separate from definition so a call site can name a callee
//! whose body does not exist yet. That is not a convenience — mutual recursion
//! needs it, and so does the stackifier, which computes every function's frame
//! size before emitting any of them.

use evm_isa::Fork;

use super::IrError;
use super::entity::Func;
use super::func::Function;
use super::types::Signature;

/// A module targets one fork, because the assembler does.
pub struct Module {
    pub fork: Fork,
    names: Vec<Box<str>>,
    sigs: Vec<Signature>,
    /// `None` for a function that has been declared but not defined.
    funcs: Vec<Option<Function>>,
    entry: Option<Func>,
}

impl Module {
    pub fn new(fork: Fork) -> Self {
        Module {
            fork,
            names: Vec::new(),
            sigs: Vec::new(),
            funcs: Vec::new(),
            entry: None,
        }
    }

    /// Reserve a handle and a signature. The body may come later, or never —
    /// [`verify`](Self::verify) is what insists it eventually arrives.
    pub fn declare(&mut self, name: &str, sig: Signature) -> Func {
        let func = Func::new(self.names.len());
        self.names.push(name.into());
        self.sigs.push(sig);
        self.funcs.push(None);
        func
    }

    /// Attach a body to a declaration.
    ///
    /// Fails if the signature disagrees with the one declared, or if a body is
    /// already present — both are the kind of mistake that would otherwise
    /// surface as a caller writing arguments into the wrong frame slots.
    pub fn define(&mut self, func: Func, body: Function) -> Result<(), IrError> {
        if body.sig != self.sigs[func.index()] {
            return Err(IrError::SignatureMismatch {
                func,
                name: self.names[func.index()].clone(),
            });
        }
        if self.funcs[func.index()].is_some() {
            return Err(IrError::AlreadyDefined {
                func,
                name: self.names[func.index()].clone(),
            });
        }
        self.funcs[func.index()] = Some(body);
        Ok(())
    }

    /// The function execution begins at.
    pub fn set_entry(&mut self, func: Func) {
        self.entry = Some(func);
    }

    pub fn entry(&self) -> Option<Func> {
        self.entry
    }

    pub fn signature(&self, func: Func) -> &Signature {
        &self.sigs[func.index()]
    }

    pub fn name(&self, func: Func) -> &str {
        &self.names[func.index()]
    }

    /// The body, or `None` if only declared.
    pub fn get(&self, func: Func) -> Option<&Function> {
        self.funcs[func.index()].as_ref()
    }

    pub fn get_mut(&mut self, func: Func) -> Option<&mut Function> {
        self.funcs[func.index()].as_mut()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Every declared function, in declaration order.
    pub fn funcs(&self) -> impl Iterator<Item = Func> + '_ {
        (0..self.names.len()).map(Func::new)
    }

    pub fn has_func(&self, func: Func) -> bool {
        func.index() < self.names.len()
    }

    /// Whether every declaration has a body and an entry point is set.
    ///
    /// Per-function checking arrives with `ir::verify` in the next step; this
    /// is the module-level half.
    pub fn verify(&self) -> Result<(), IrError> {
        for func in self.funcs() {
            if self.get(func).is_none() {
                return Err(IrError::Undefined {
                    func,
                    name: self.names[func.index()].clone(),
                });
            }
        }
        if self.entry.is_none() {
            return Err(IrError::NoEntry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
