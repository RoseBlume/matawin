use std::collections::HashMap;
use super::BitReader;

pub struct Huffman {
    table: HashMap<u32, (u16, u8)>, // code -> (symbol, bit_length)
}

impl Huffman {
    pub fn new(code_lengths: &[u8]) -> Self {
        let mut bl_count = [0u16; 16];
        for &len in code_lengths {
            if len > 0 {
                bl_count[len as usize] += 1;
            }
        }

        let mut next_code = [0u32; 16];
        let mut code = 0;

        for bits in 1..16 {
            code = (code + bl_count[bits - 1] as u32) << 1;
            next_code[bits] = code;
        }

        let mut table = HashMap::new();

        for (symbol, &len) in code_lengths.iter().enumerate() {
            if len != 0 {
                let c = next_code[len as usize];
                next_code[len as usize] += 1;
                table.insert(c, (symbol as u16, len));
            }
        }

        Self { table }
    }

    pub fn decode(&self, br: &mut BitReader) -> u16 {
        let mut code = 0;

        for len in 1..16 {
            code |= br.read_bits(1) << (len - 1);

            if let Some(&(symbol, l)) = self.table.get(&code) {
                if l as usize == len {
                    return symbol;
                }
            }
        }

        panic!("Invalid Huffman code");
    }
}