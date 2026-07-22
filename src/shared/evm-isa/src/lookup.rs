use crate::spec::OpSpec;
use crate::table::OPCODES;

/// Linear scan over ~150 entries — fine at this size, and keeps the crate at
/// zero dependencies (no `HashMap`, no `phf`).
pub const fn by_mnemonic(name: &str) -> Option<&'static OpSpec> {
    let needle = name.as_bytes();
    let mut i = 0;
    while i < OPCODES.len() {
        if str_eq(OPCODES[i].mnemonic.as_bytes(), needle) {
            return Some(&OPCODES[i]);
        }
        i += 1;
    }
    None
}

pub const fn by_byte(b: u8) -> Option<&'static OpSpec> {
    let mut i = 0;
    while i < OPCODES.len() {
        if OPCODES[i].byte == b {
            return Some(&OPCODES[i]);
        }
        i += 1;
    }
    None
}

/// `str::eq` isn't const-stable; a byte-by-byte loop is.
const fn str_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
