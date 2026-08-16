// SPDX-License-Identifier: MIT OR Apache-2.0

//! String Interning and Symbol Management
//!
//! This module provides the [`SymbolRegistry`], a central mechanism for string interning.
//!
//! # Why Interning?
//! Identifiers (variable names, function names) appear repeatedly.
//! Storing these as full `String` objects in every `Token` is memory-intensive and
//! makes AST nodes bulky.
//!
//! The [`SymbolRegistry`] solves this by:
//! 1. Storing each unique string exactly once.
//! 2. Assigning each unique string a lightweight [`Symbol`] (a 4-byte ID).
//! 3. Allowing O(1) comparisons between identifiers by comparing integer IDs.
//!
//! This registry acts as the "source of truth" for what a [`Symbol`] represents.

use std::{collections::HashMap, sync::Arc};

/// A unique identifier for a string stored in the [`SymbolRegistry`].
///
/// Symbols are lightweight (4-byte) handles that can be easily copied and compared,
/// making them ideal for use in an Abstract Syntax Tree (AST) or token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Symbol(u32);

/// A central registry for string interning.
///
/// The `SymbolRegistry` interns strings and assigns them stable [`Symbol`] IDs.
/// It uses `Arc<str>` to ensure each unique string is stored in memory exactly
/// once, while allowing both the reverse lookup table and the primary mapping
/// to share the same heap-allocated memory.
///
/// # Example
/// ```
/// # use nyx_token::SymbolRegistry;
/// let mut registry = SymbolRegistry::new();
/// let sym = registry.intern("variable_name");
/// assert_eq!(registry.resolve(sym), "variable_name");
/// ```
#[derive(Debug, Default)]
pub struct SymbolRegistry {
    /// Maps the actual string to its unique [`Symbol`].
    map: HashMap<Arc<str>, Symbol>,

    /// Stores the strings in order, where the index corresponds to the [`Symbol`] ID.
    strings: Vec<Arc<str>>,
}

impl SymbolRegistry {
    /// Creates a new, empty [`SymbolRegistry`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a string, returning its corresponding [`Symbol`].
    ///
    /// If the string is already present in the registry, the existing [`Symbol`]
    /// is returned. If it is new, it is added to the registry and a new
    /// [`Symbol`] is assigned and returned.
    pub fn intern(&mut self, value: &str) -> Symbol {
        if let Some(symbol) = self.get(value) {
            return symbol;
        }

        let id = u32::try_from(self.strings.len())
            .expect("symbol registry exceeded the maximum number of symbols");
        let symbol = Symbol(id);

        let shared: Arc<str> = Arc::from(value);
        self.strings.push(Arc::clone(&shared));
        self.map.insert(shared, symbol);

        symbol
    }

    /// Returns the [`Symbol`] associated with `value`, if it has been interned.
    pub fn get(&self, value: &str) -> Option<Symbol> {
        self.map.get(value).copied()
    }

    /// Resolves a [`Symbol`] back into its original string slice.
    ///
    /// # Panics
    ///
    /// Panics if the [`Symbol`] does not correspond to a string in this registry,
    /// such as when it was created by a different registry.
    pub fn resolve(&self, symbol: Symbol) -> &str {
        &self.strings[symbol.0 as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_registry_lifecycle() {
        let mut registry = SymbolRegistry::new();

        let sym_a = registry.intern("var_a");
        let sym_b = registry.intern("var_b");

        assert_ne!(sym_a, sym_b, "Symbols for different strings must be unique");

        assert_eq!(registry.get("var_a"), Some(sym_a));
        assert_eq!(registry.get("var_b"), Some(sym_b));
        assert_eq!(registry.get("unknown"), None);

        let sym_a_again = registry.intern("var_a");
        assert_eq!(
            sym_a, sym_a_again,
            "Storing same string should return same symbol"
        );

        assert_eq!(registry.resolve(sym_a), "var_a");
        assert_eq!(registry.resolve(sym_b), "var_b");
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_resolve_invalid_symbol() {
        let registry = SymbolRegistry::new();
        registry.resolve(Symbol(999));
    }
}
