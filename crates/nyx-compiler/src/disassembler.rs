use std::fmt::{self, Write};

use nyx_token::SymbolRegistry;

use crate::{
    ByteCode, OpCode, Value,
    opcodes::{BinaryOpCode, UnaryOpCode},
};

pub struct Disassembler<'a> {
    bytecode: &'a ByteCode,
    symbols: &'a SymbolRegistry,
}

impl<'a> Disassembler<'a> {
    pub fn new(bytecode: &'a ByteCode, symbols: &'a SymbolRegistry) -> Self {
        Self { bytecode, symbols }
    }

    fn disassemble_instruction(
        &self,
        output: &mut String,
        offset: usize,
    ) -> Result<usize, fmt::Error> {
        let byte = self.bytecode.code()[offset];

        let Some(opcode) = OpCode::from_byte(byte) else {
            writeln!(output, "{offset:4}  {:<18} 0x{byte:02X}", "UNKNOWN")?;

            return Ok(offset + 1);
        };

        match opcode {
            OpCode::LoadConstant => {
                let index = self.read_u16(offset + 1);

                match self.bytecode.constants().get(index as usize) {
                    Some(value) => match value {
                        Value::Ident(symbol) => {
                            let name = self.symbols.resolve(*symbol);

                            writeln!(output, "{offset:4}  {:<18} {:5}  ({name})", opcode, index,)?;
                        }

                        _ => {
                            writeln!(
                                output,
                                "{offset:4}  {:<18} {:5}  ({value:?})",
                                opcode, index,
                            )?;
                        }
                    },

                    None => {
                        writeln!(
                            output,
                            "{offset:4}  {:<18} {:5}  (<invalid constant>)",
                            opcode, index,
                        )?;
                    }
                }

                Ok(offset + 3)
            }

            OpCode::DefineGlobal | OpCode::LoadGlobal | OpCode::StoreGlobal => {
                let index = self.read_u16(offset + 1);

                let symbol = self
                    .bytecode
                    .globals()
                    .iter()
                    .find_map(|(symbol, slot)| (*slot == index).then_some(*symbol));

                match symbol {
                    Some(symbol) => {
                        let name = self.symbols.resolve(symbol);

                        writeln!(output, "{offset:4}  {:<18} {:5}  ({name})", opcode, index,)?;
                    }

                    None => {
                        writeln!(
                            output,
                            "{offset:4}  {:<18} {:5}  (<unknown global>)",
                            opcode, index,
                        )?;
                    }
                }

                Ok(offset + 3)
            }

            OpCode::DefineLocal | OpCode::LoadLocal | OpCode::StoreLocal => {
                let index = self.read_u16(offset + 1);

                writeln!(output, "{offset:4}  {:<18} {:5}", opcode, index,)?;

                Ok(offset + 3)
            }

            OpCode::Binary => {
                let code = self.read_u8(offset + 1);

                let op = BinaryOpCode::from_byte(code)
                    .map_or_else(|| format!("Unknown({code})"), |op| op.to_string());

                writeln!(output, "{offset:4}  {:<18} {:5}  ({op})", opcode, code)?;

                Ok(offset + 2)
            }

            OpCode::Unary => {
                let code = self.read_u8(offset + 1);

                let op = UnaryOpCode::from_byte(code)
                    .map_or_else(|| format!("Unknown({code})"), |op| op.to_string());

                writeln!(output, "{offset:4}  {:<18} {:5}  ({op})", opcode, code)?;

                Ok(offset + 2)
            }
        }
    }

    fn read_u16(&self, offset: usize) -> u16 {
        let code = self.bytecode.code();

        u16::from_le_bytes([code[offset], code[offset + 1]])
    }

    fn read_u8(&self, offset: usize) -> u8 {
        self.bytecode.code()[offset]
    }
}

impl fmt::Display for Disassembler<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = String::new();

        writeln!(output, "Constants:")?;

        for (index, value) in self.bytecode.constants().iter().enumerate() {
            match value {
                Value::Ident(symbol) => {
                    let name = self.symbols.resolve(*symbol);
                    writeln!(output, "{index:4}  Ident({name})")?;
                }

                _ => {
                    writeln!(output, "{index:4}  {value:?}")?;
                }
            }
        }

        writeln!(output)?;
        writeln!(output, "Globals:")?;

        let mut globals: Vec<_> = self.bytecode.globals().iter().collect();

        globals.sort_by_key(|(_, index)| **index);

        for (symbol, index) in globals {
            let name = self.symbols.resolve(*symbol);

            writeln!(output, "{index:4}  {name}")?;
        }

        writeln!(output)?;
        writeln!(output, "Bytecode:")?;

        let mut offset = 0;

        while offset < self.bytecode.code().len() {
            offset = self.disassemble_instruction(&mut output, offset)?;
        }

        f.write_str(&output)
    }
}
