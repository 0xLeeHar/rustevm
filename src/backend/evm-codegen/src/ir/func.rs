//! A function: three arenas and the blocks that index into them.
//!
//! Everything is a `Vec` keyed by an entity index. That is not just an
//! implementation convenience — the naive stackifier derives a value's frame
//! slot straight from its index, so **the value arena *is* the slot map**, and
//! nothing has to allocate or record one.
//!
//! It also fixes the calling convention's other half: a function's entry block
//! takes one parameter per signature parameter, and those are created first, so
//! parameters are always values `0..P-1`. A caller can therefore write into a
//! callee's parameter slots knowing nothing about it but its handle.

use super::entity::{Block, Inst, Value};
use super::inst::{InstData, Terminator};
use super::types::{Signature, Type};

/// Where a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDef {
    Result { inst: Inst, index: u16 },
    Param { block: Block, index: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueData {
    pub ty: Type,
    pub def: ValueDef,
}

/// What a block is for.
///
/// Advisory only — nothing in step 2 reads it. It exists so a future `evm-opt`
/// can place cold blocks out of line, and so that the rule in
/// [`effect`](super::effect) has something to name: reaching a `Panic` block is
/// never "work that can be removed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockKind {
    #[default]
    Normal,
    /// A cold block that exists to revert.
    Panic,
}

#[derive(Debug, Clone, Default)]
pub struct BlockData {
    pub params: Vec<Value>,
    /// Straight-line body. Terminators are not in here.
    pub insts: Vec<Inst>,
    /// `None` until the block is finished; `verify` rejects any that stay so.
    pub term: Option<Terminator>,
    pub kind: BlockKind,
}

/// One function body.
///
/// Construct through [`FunctionBuilder`](super::builder::FunctionBuilder)
/// rather than directly — the arenas have to stay consistent with each other,
/// and the builder is the only thing that guarantees it.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: Box<str>,
    pub sig: Signature,
    entry: Block,
    insts: Vec<InstData>,
    values: Vec<ValueData>,
    blocks: Vec<BlockData>,
}

impl Function {
    /// A function with an entry block carrying one parameter per signature
    /// parameter.
    pub(crate) fn new(name: &str, sig: Signature) -> Self {
        let mut func = Function {
            name: name.into(),
            sig: sig.clone(),
            entry: Block::new(0),
            insts: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
        };
        let entry = func.create_block();
        debug_assert_eq!(entry, func.entry, "the entry block is always block0");
        for ty in &sig.params {
            func.append_param(entry, *ty);
        }
        func
    }

    // --- construction, all crate-internal ---

    pub(crate) fn create_block(&mut self) -> Block {
        let block = Block::new(self.blocks.len());
        self.blocks.push(BlockData::default());
        block
    }

    pub(crate) fn append_param(&mut self, block: Block, ty: Type) -> Value {
        let index = self.blocks[block.index()].params.len() as u16;
        let value = self.push_value(ValueData {
            ty,
            def: ValueDef::Param { block, index },
        });
        self.blocks[block.index()].params.push(value);
        value
    }

    /// Append an instruction to `block`, minting one value per result.
    ///
    /// `result_types` is passed in rather than derived because checked
    /// arithmetic's second result is a `BOOL` regardless of the first's type,
    /// and a `Call`'s results come from the callee's signature.
    pub(crate) fn append_inst(&mut self, block: Block, mut data: InstData, result_types: &[Type]) -> Inst {
        let inst = Inst::new(self.insts.len());
        data.results = Vec::with_capacity(result_types.len());
        // Push the instruction before its results so that `ValueDef::Result`
        // can name it.
        self.insts.push(data);
        for (index, ty) in result_types.iter().enumerate() {
            let value = self.push_value(ValueData {
                ty: *ty,
                def: ValueDef::Result {
                    inst,
                    index: index as u16,
                },
            });
            self.insts[inst.index()].results.push(value);
        }
        self.blocks[block.index()].insts.push(inst);
        inst
    }

    pub(crate) fn set_terminator(&mut self, block: Block, term: Terminator) {
        self.blocks[block.index()].term = Some(term);
    }

    pub(crate) fn set_block_kind(&mut self, block: Block, kind: BlockKind) {
        self.blocks[block.index()].kind = kind;
    }

    pub(crate) fn set_note(&mut self, inst: Inst, note: &str) {
        self.insts[inst.index()].note = Some(note.into());
    }

    fn push_value(&mut self, data: ValueData) -> Value {
        let value = Value::new(self.values.len());
        self.values.push(data);
        value
    }

    // --- reading ---

    pub fn entry(&self) -> Block {
        self.entry
    }

    pub fn inst(&self, inst: Inst) -> &InstData {
        &self.insts[inst.index()]
    }

    /// For `evm-opt`: the one mutation that mask elision needs.
    pub fn inst_mut(&mut self, inst: Inst) -> &mut InstData {
        &mut self.insts[inst.index()]
    }

    pub fn results(&self, inst: Inst) -> &[Value] {
        &self.insts[inst.index()].results
    }

    pub fn args(&self, inst: Inst) -> &[Value] {
        &self.insts[inst.index()].args
    }

    pub fn value(&self, value: Value) -> ValueData {
        self.values[value.index()]
    }

    pub fn value_type(&self, value: Value) -> Type {
        self.values[value.index()].ty
    }

    pub fn value_def(&self, value: Value) -> ValueDef {
        self.values[value.index()].def
    }

    pub fn block(&self, block: Block) -> &BlockData {
        &self.blocks[block.index()]
    }

    pub fn block_params(&self, block: Block) -> &[Value] {
        &self.blocks[block.index()].params
    }

    pub fn terminator(&self, block: Block) -> Option<&Terminator> {
        self.blocks[block.index()].term.as_ref()
    }

    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn num_insts(&self) -> usize {
        self.insts.len()
    }

    /// Every block, in index order. The entry block is first.
    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        (0..self.blocks.len()).map(Block::new)
    }

    /// Whether `value` exists in this function's arena.
    pub fn has_value(&self, value: Value) -> bool {
        value.index() < self.values.len()
    }

    pub fn has_block(&self, block: Block) -> bool {
        block.index() < self.blocks.len()
    }
}

#[cfg(test)]
mod tests;
