use std::collections::HashMap;

use crate::Value;
use crate::{
    OpCode,
    opcodes::{BinaryOpCode, UnaryOpCode},
};
use nyx_token::Symbol;

/// A hashable representation of a [`Value`] used for constant-pool deduplication.
///
/// `ConstKey` defines how values are identified inside the constant pool. Unlike
/// [`Value`], it can implement [`Eq`] and [`Hash`] because floating-point values
/// are represented by their raw [`f64`] bit pattern.
///
/// This allows constants to be used as keys in a hash map while keeping
/// constant-pool identity separate from the runtime equality semantics of
/// [`Value`].
///
/// # Floating-point values
///
/// [`f64`] does not implement [`Eq`] or [`Hash`] because of special values such
/// as NaN. To make floats suitable for use as constant keys, they are converted
/// to their raw `u64` representation using [`f64::to_bits`].
///
/// As a consequence, floating-point constants are considered identical only
/// when their bit representations are identical. For example, `0.0` and `-0.0`
/// produce different keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ConstKey {
    /// An integer constant.
    Int(i64),

    /// A floating-point constant represented by its raw IEEE 754 bit pattern.
    Float(u64),

    /// A boolean constant.
    Bool(bool),

    /// An interned identifier.
    Ident(Symbol),
}

impl From<Value> for ConstKey {
    /// Converts a runtime [`Value`] into its constant-pool key representation.
    ///
    /// Integer, boolean, and identifier values retain their underlying values.
    /// Floating-point values are converted to their raw bit representation so
    /// that the resulting key can safely implement [`Eq`] and [`Hash`].
    fn from(value: Value) -> Self {
        match value {
            Value::Int(v) => ConstKey::Int(v),
            Value::Float(v) => ConstKey::Float(v.to_bits()),
            Value::Bool(v) => ConstKey::Bool(v),
            Value::Ident(v) => ConstKey::Ident(v),
        }
    }
}

/// Stores bytecode constants and deduplicates repeated values.
///
/// `ConstPool` maintains constants in insertion order while using a lookup table
/// to ensure that equivalent constants are stored only once.
///
/// The [`Vec`] provides index-based access to constants, which is useful for
/// bytecode instructions that refer to constants by numeric index. The
/// [`HashMap`] provides efficient lookup from a [`ConstKey`] to the existing
/// constant index.
///
/// # Indexing
///
/// Constant indices are represented as `u16`, so the pool supports at most
/// `u16::MAX + 1` distinct constants. Attempting to add more constants causes
/// [`ConstPool::add`] to panic.
///
/// # Deduplication
///
/// When a value is added, it is converted into a [`ConstKey`]. If the key is
/// already present in the lookup table, the existing index is returned and the
/// value is not inserted again.
///
/// This keeps bytecode compact while preserving the insertion order of unique
/// constants.
#[derive(Debug, Default, Clone)]
struct ConstPool {
    /// Unique constants stored in the order they were first inserted.
    ///
    /// The position of each value corresponds to the `u16` index used by
    /// bytecode instructions.
    constants: Vec<Value>,

    /// Maps each constant's canonical key to its index in [`Self::constants`].
    lookup: HashMap<ConstKey, u16>,
}

impl ConstPool {
    /// Adds a constant to the pool and returns its index.
    ///
    /// If an equivalent constant already exists, its existing index is returned
    /// and the value is not inserted again.
    ///
    /// # Panics
    ///
    /// Panics if the number of unique constants exceeds the range representable
    /// by `u16`.
    fn add(&mut self, value: Value) -> u16 {
        let key = ConstKey::from(value);

        if let Some(&index) = self.lookup.get(&key) {
            return index;
        }

        let index = u16::try_from(self.constants.len())
            .expect("constant pool exceeds maximum supported size");

        self.constants.push(value);
        self.lookup.insert(key, index);

        index
    }

    /// Returns all unique constants currently stored in the pool.
    ///
    /// Constants are returned in the order in which they were first inserted.
    fn constants(&self) -> &[Value] {
        self.constants.as_slice()
    }
}

/// Compiled Nyx bytecode and its associated constant pool.
#[derive(Debug, Default, Clone)]
pub struct ByteCode {
    /// Encoded bytecode instructions and operands.
    code: Vec<u8>,

    /// Constants referenced by bytecode instructions.
    const_pool: ConstPool,

    /// Maps interned global variable names to their assigned runtime slots.
    ///
    /// The compiler uses this table to resolve references to global variables.
    /// The stored `u16` is the slot encoded by global bytecode instructions such
    /// as [`OpCode::DefineGlobal`], [`OpCode::LoadGlobal`], and
    /// [`OpCode::StoreGlobal`].
    ///
    /// The actual values of global variables are stored by the virtual machine at
    /// runtime rather than in this map.
    globals: HashMap<Symbol, u16>,
}

impl ByteCode {
    /// Returns the encoded bytecode instruction stream.
    pub fn code(&self) -> &[u8] {
        self.code.as_slice()
    }

    /// Returns the constant pool in index order.
    ///
    /// Bytecode instructions such as [`OpCode::LoadConstant`] use an index
    /// into this slice to reference constants.
    pub fn constants(&self) -> &[Value] {
        self.const_pool.constants()
    }

    /// Returns the mapping from interned global variable names to their
    /// assigned runtime slots.
    ///
    /// The slot values correspond to the operands used by global-variable
    /// instructions such as [`OpCode::DefineGlobal`], [`OpCode::LoadGlobal`],
    /// and [`OpCode::StoreGlobal`].
    pub fn globals(&self) -> &HashMap<Symbol, u16> {
        &self.globals
    }

    /// Adds a value to the constant pool and returns its index.
    ///
    /// # Panics
    ///
    /// Panics if the constant pool already contains the maximum number of
    /// constants addressable by a `u16`.
    pub(crate) fn store_const(&mut self, value: Value) -> u16 {
        self.const_pool.add(value)
    }

    /// Registers a global variable and returns its runtime slot.
    ///
    /// If the global has already been registered, its existing slot is returned.
    /// Otherwise, a new slot is allocated.
    ///
    /// # Panics
    ///
    /// Panics if the global table already contains the maximum number of globals
    /// addressable by a `u16`.
    pub(crate) fn register_global(&mut self, symbol: Symbol) -> u16 {
        if let Some(&slot) = self.globals.get(&symbol) {
            return slot;
        }

        let slot =
            u16::try_from(self.globals.len()).expect("global table exceeds maximum supported size");

        self.globals.insert(symbol, slot);

        slot
    }
    /// Emits a `LoadConstant` instruction for the given constant-pool index.
    ///
    /// The constant index is encoded as a two-byte operand immediately
    /// following the opcode.
    pub(crate) fn emit_load_constant(&mut self, index: u16) {
        self.emit_opcode(OpCode::LoadConstant);
        self.emit_u16(index);
    }

    /// Emits a `DefineGlobal` instruction for the given global slot.
    ///
    /// The global index is encoded as a two-byte operand immediately
    /// following the opcode.
    pub(crate) fn emit_define_global(&mut self, index: u16) {
        self.emit_opcode(OpCode::DefineGlobal);
        self.emit_u16(index);
    }

    /// Emits a `LoadGlobal` instruction for the given global slot.
    ///
    /// The global index is encoded as a two-byte operand immediately
    /// following the opcode.
    pub(crate) fn emit_load_global(&mut self, index: u16) {
        self.emit_opcode(OpCode::LoadGlobal);
        self.emit_u16(index);
    }

    /// Emits a `StoreGlobal` instruction for the given global slot.
    ///
    /// The global index is encoded as a two-byte operand immediately
    /// following the opcode.
    #[allow(dead_code)]
    pub(crate) fn emit_store_global(&mut self, index: u16) {
        self.emit_opcode(OpCode::StoreGlobal);
        self.emit_u16(index);
    }

    /// Emits a binary operation and its specific opcode.
    pub(crate) fn emit_binary_opcode(&mut self, opcode: BinaryOpCode) {
        self.code.push(OpCode::Binary as u8);
        self.code.push(opcode as u8);
    }

    /// Emits a unary operation and its specific opcode.
    pub(crate) fn emit_unary_opcode(&mut self, opcode: UnaryOpCode) {
        self.code.push(OpCode::Unary as u8);
        self.code.push(opcode as u8);
    }

    /// Appends a single opcode to the bytecode stream.
    pub(crate) fn emit_opcode(&mut self, opcode: OpCode) {
        self.code.push(opcode as u8);
    }

    /// Appends a `u16` operand in little-endian byte order.
    pub(crate) fn emit_u16(&mut self, value: u16) {
        self.code.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_token::SymbolRegistry;
    use rstest::rstest;

    #[test]
    fn test_bytecode_default() {
        let bytecode = ByteCode::default();

        assert!(bytecode.code().is_empty());
        assert!(bytecode.constants().is_empty());
        assert!(bytecode.globals().is_empty());
    }

    #[test]
    fn test_bytecode_store_constant() {
        // Make few symbols
        let mut registry = SymbolRegistry::new();
        let name = registry.intern("name");
        let age = registry.intern("age");

        let constants = vec![
            Value::Int(10),
            Value::Float(39.0),
            Value::Ident(name),
            Value::Bool(true),
            Value::Bool(false),
            Value::Ident(age),
        ];

        let mut bytecode = ByteCode::default();

        for item in constants.iter().copied() {
            bytecode.store_const(item);
        }

        assert_eq!(bytecode.constants(), constants.as_slice());
        assert!(bytecode.code().is_empty());
        assert!(bytecode.globals().is_empty());
    }

    #[test]
    fn test_same_constants_are_stored_once() {
        let mut registry = SymbolRegistry::new();
        let name = registry.intern("name");

        let constants = vec![
            Value::Int(10),
            Value::Int(10),
            Value::Ident(name),
            Value::Float(39.203),
            Value::Ident(name),
            Value::Float(39.203),
            Value::Float(39.203),
            Value::Float(39.202),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
        ];

        let mut bytecode = ByteCode::default();

        for item in constants.iter().copied() {
            bytecode.store_const(item);
        }

        let expected = vec![
            Value::Int(10),
            Value::Ident(name),
            Value::Float(39.203),
            Value::Float(39.202),
            Value::Bool(true),
            Value::Bool(false),
        ];

        assert_eq!(bytecode.constants(), expected.as_slice());
    }

    #[test]
    fn test_register_global() {
        let mut registry = SymbolRegistry::new();
        let name = registry.intern("name");
        let age = registry.intern("age");

        let mut bytecode = ByteCode::default();

        let name_slot = bytecode.register_global(name);
        let age_slot = bytecode.register_global(age);

        assert_eq!(name_slot, 0);
        assert_eq!(age_slot, 1);

        assert_eq!(bytecode.globals().get(&name), Some(&0));
        assert_eq!(bytecode.globals().get(&age), Some(&1));

        assert_eq!(bytecode.globals().len(), 2);
    }

    #[test]
    fn test_register_same_global_returns_existing_slot() {
        let mut registry = SymbolRegistry::new();
        let name = registry.intern("name");

        let mut bytecode = ByteCode::default();

        let first = bytecode.register_global(name);
        let second = bytecode.register_global(name);

        assert_eq!(first, second);
        assert_eq!(first, 0);

        assert_eq!(bytecode.globals().len(), 1);
        assert_eq!(bytecode.globals().get(&name), Some(&0));
    }

    #[test]
    fn test_emit_load_constant() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_load_constant(42);

        assert_eq!(bytecode.code(), &[OpCode::LoadConstant as u8, 42, 0,]);
    }

    #[test]
    fn test_emit_u16_uses_little_endian() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_u16(0x1234);

        assert_eq!(bytecode.code(), &[0x34, 0x12]);
    }

    #[test]
    fn test_emit_define_global() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_define_global(0x1234);

        assert_eq!(bytecode.code(), &[OpCode::DefineGlobal as u8, 0x34, 0x12,]);
    }

    #[test]
    fn test_emit_load_global() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_load_global(0x1234);

        assert_eq!(bytecode.code(), &[OpCode::LoadGlobal as u8, 0x34, 0x12,]);
    }

    #[test]
    fn test_emit_store_global() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_store_global(0x1234);

        assert_eq!(bytecode.code(), &[OpCode::StoreGlobal as u8, 0x34, 0x12,]);
    }

    #[test]
    fn test_emit_opcode() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_opcode(OpCode::LoadConstant);
        bytecode.emit_opcode(OpCode::DefineGlobal);

        assert_eq!(
            bytecode.code(),
            &[OpCode::LoadConstant as u8, OpCode::DefineGlobal as u8,]
        );
    }

    #[test]
    fn test_emitted_instructions_preserve_order() {
        let mut bytecode = ByteCode::default();

        bytecode.emit_load_constant(3);
        bytecode.emit_define_global(7);
        bytecode.emit_load_global(7);

        assert_eq!(
            bytecode.code(),
            &[
                OpCode::LoadConstant as u8,
                3,
                0,
                OpCode::DefineGlobal as u8,
                7,
                0,
                OpCode::LoadGlobal as u8,
                7,
                0,
            ]
        );
    }

    #[rstest]
    #[case(BinaryOpCode::Add)]
    #[case(BinaryOpCode::Sub)]
    #[case(BinaryOpCode::Mul)]
    #[case(BinaryOpCode::Div)]
    fn test_emit_binary_opcode(#[case] opcode: BinaryOpCode) {
        let mut bytecode = ByteCode::default();

        bytecode.emit_binary_opcode(opcode);

        assert_eq!(bytecode.code(), &[OpCode::Binary as u8, opcode as u8]);
    }

    #[rstest]
    #[case(UnaryOpCode::Neg)]
    #[case(UnaryOpCode::Not)]
    fn test_emit_unary_opcode(#[case] opcode: UnaryOpCode) {
        let mut bytecode = ByteCode::default();

        bytecode.emit_unary_opcode(opcode);

        assert_eq!(bytecode.code(), &[OpCode::Unary as u8, opcode as u8]);
    }
}
