//! VP8L canonical Huffman decoder.
//!
//! This is kept separate from the WebP container/parser code so the crate has
//! one maintained VP8L Huffman implementation instead of an older hash-map
//! implementation plus a second optimized inline implementation.

use super::BitReader;
use crate::ImageError;

const HUFF_ROOT_BITS: usize = 8;
const HUFF_ROOT_SIZE: usize = 1 << HUFF_ROOT_BITS;
const HUFF_NONE: u16 = u16::MAX;

#[derive(Debug, Clone, Copy)]
/// Represents one entry in a WebP Huffman lookup table.
struct HuffEntry {
    /// Stores either the decoded symbol or an internal lookup value for this Huffman root entry.
    value: u16,
    /// Stores the number of bits consumed when this Huffman lookup entry is selected.
    bits: u8,
    /// Records whether this root-table entry resolves directly to a Huffman symbol.
    is_symbol: bool,
}

const EMPTY_HUFF_ENTRY: HuffEntry = HuffEntry {
    value: HUFF_NONE,
    bits: 0,
    is_symbol: false,
};

#[derive(Debug, Clone, Copy)]
/// Represents one node in a WebP Huffman decoding tree.
struct HuffNode {
    /// Stores the Huffman symbol represented by this node when it is a leaf.
    symbol: u16,
    /// Stores the child-node indices selected by zero and one input bits.
    child: [u16; 2],
}

impl HuffNode {
    #[inline]
    /// Constructs and initializes a new instance of this type.
    fn new() -> Self {
        Self { symbol: HUFF_NONE, child: [HUFF_NONE; 2] }
    }
}

/// VP8L Huffman decoder with an 8-bit direct lookup front-end.
///
/// Most symbols resolve with one table lookup and one bit-buffer shift. Codes
/// longer than eight bits continue through a compact flat trie, avoiding the
/// heap pointer chasing of a `Box`-per-node tree.
#[derive(Debug, Clone)]
pub(super) struct Huffman {
    /// Stores the fixed-size root lookup table used for fast Huffman decoding.
    root: [HuffEntry; HUFF_ROOT_SIZE],
    /// Stores secondary Huffman tree nodes reached when a code exceeds the root lookup width.
    nodes: Vec<HuffNode>,
    /// Stores the sole symbol when the Huffman alphabet contains only one coded value.
    single: Option<u16>,
    /// Number of root bits actually required by this alphabet, capped at eight.
    /// Small distance/channel alphabets therefore avoid demanding an eight-bit
    /// refill when all of their codes are shorter.
    lookup_bits: u8,
}

impl Huffman {
    /// Builds a canonical Huffman decoder from the supplied code lengths.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub(super) fn from_lengths(lengths: &[u8]) -> Result<Self, ImageError> {
        let mut symbol_count = 0usize;
        let mut only_symbol = 0u16;
        let mut max_len = 0usize;
        let mut counts = [0u32; 16];

        for (symbol, &len) in lengths.iter().enumerate() {
            if len == 0 { continue; }
            if len > 15 {
                return Err(ImageError::new("VP8L Huffman code length exceeds 15"));
            }
            symbol_count += 1;
            only_symbol = symbol as u16;
            max_len = max_len.max(len as usize);
            counts[len as usize] += 1;
        }

        if symbol_count == 0 {
            return Err(ImageError::new("VP8L Huffman code has no symbols"));
        }
        if symbol_count == 1 {
            return Ok(Self {
                root: [EMPTY_HUFF_ENTRY; HUFF_ROOT_SIZE],
                nodes: Vec::new(),
                single: Some(only_symbol),
                lookup_bits: 0,
            });
        }

        let mut left = 1i32;
        for len in 1..=max_len {
            left = (left << 1) - counts[len] as i32;
            if left < 0 {
                return Err(ImageError::new("Oversubscribed VP8L Huffman tree"));
            }
        }
        if left != 0 {
            return Err(ImageError::new("Incomplete VP8L Huffman tree"));
        }

        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for len in 1..=max_len {
            code = (code + counts[len - 1]) << 1;
            next_code[len] = code;
        }

        let mut nodes = Vec::with_capacity(symbol_count * 2);
        nodes.push(HuffNode::new());

        for (symbol, &len_u8) in lengths.iter().enumerate() {
            if len_u8 == 0 { continue; }
            let len = len_u8 as usize;
            let canonical = next_code[len];
            next_code[len] += 1;
            let mut node_index = 0usize;

            for bit_index in 0..len {
                let bit = ((canonical >> (len - 1 - bit_index)) & 1) as usize;
                let child = nodes[node_index].child[bit];
                let next = if child == HUFF_NONE {
                    if nodes.len() >= HUFF_NONE as usize {
                        return Err(ImageError::new("VP8L Huffman tree is too large"));
                    }
                    let index = nodes.len() as u16;
                    nodes.push(HuffNode::new());
                    nodes[node_index].child[bit] = index;
                    index
                } else {
                    child
                };
                node_index = next as usize;

                if bit_index + 1 == len {
                    if nodes[node_index].symbol != HUFF_NONE
                        || nodes[node_index].child[0] != HUFF_NONE
                        || nodes[node_index].child[1] != HUFF_NONE
                    {
                        return Err(ImageError::new("Invalid VP8L Huffman tree"));
                    }
                    nodes[node_index].symbol = symbol as u16;
                } else if nodes[node_index].symbol != HUFF_NONE {
                    return Err(ImageError::new("Invalid VP8L Huffman prefix"));
                }
            }
        }

        let mut root = [EMPTY_HUFF_ENTRY; HUFF_ROOT_SIZE];
        for key in 0..HUFF_ROOT_SIZE {
            let mut node = 0usize;
            let mut consumed = 0usize;
            let mut valid = true;

            while consumed < HUFF_ROOT_BITS && nodes[node].symbol == HUFF_NONE {
                let bit = (key >> consumed) & 1;
                let child = nodes[node].child[bit];
                if child == HUFF_NONE {
                    valid = false;
                    break;
                }
                node = child as usize;
                consumed += 1;
            }

            if !valid { continue; }
            if nodes[node].symbol != HUFF_NONE {
                root[key] = HuffEntry {
                    value: nodes[node].symbol,
                    bits: consumed as u8,
                    is_symbol: true,
                };
            } else if consumed == HUFF_ROOT_BITS {
                root[key] = HuffEntry {
                    value: node as u16,
                    bits: HUFF_ROOT_BITS as u8,
                    is_symbol: false,
                };
            }
        }

        Ok(Self {
            root,
            nodes,
            single: None,
            lookup_bits: max_len.min(HUFF_ROOT_BITS) as u8,
        })
    }

    #[inline(always)]
    /// Decodes the supplied encoded data and returns its reconstructed representation.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub(super) fn decode(&self, bits: &mut BitReader<'_>) -> Result<u16, ImageError> {
        if let Some(symbol) = self.single {
            return Ok(symbol);
        }

        let mut node;
        let mut depth;
        let lookup_bits = self.lookup_bits as usize;
        if lookup_bits != 0 && bits.available_bits() >= lookup_bits {
            // The root table replicates shorter codes across unused high bits,
            // so indexing with only the alphabet's actual maximum short-code
            // width is valid and avoids unnecessary refills for small tables.
            let key = bits.peek_bits(lookup_bits as u32)? as usize;
            let entry = unsafe { *self.root.get_unchecked(key) };
            if entry.value == HUFF_NONE {
                return Err(ImageError::new("Invalid VP8L Huffman code"));
            }
            bits.drop_bits(entry.bits as u32);
            if entry.is_symbol {
                return Ok(entry.value);
            }
            node = entry.value as usize;
            depth = entry.bits as usize;
        } else {
            node = 0usize;
            depth = 0usize;
        }

        while depth < 15 {
            let bit = bits.read_bit()? as usize;
            let next = unsafe { self.nodes.get_unchecked(node).child[bit] };
            if next == HUFF_NONE {
                return Err(ImageError::new("Invalid VP8L Huffman code"));
            }
            node = next as usize;
            depth += 1;
            let symbol = unsafe { self.nodes.get_unchecked(node).symbol };
            if symbol != HUFF_NONE {
                return Ok(symbol);
            }
        }
        Err(ImageError::new("VP8L Huffman code is too long"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_equal_length_alphabets_build() {
        for bits in 1usize..=8 {
            let lengths = vec![bits as u8; 1usize << bits];
            let table = Huffman::from_lengths(&lengths).unwrap();
            assert_eq!(table.lookup_bits, bits as u8);
        }
    }

    #[test]
    fn malformed_huffman_lengths_fail_without_panicking() {
        assert!(Huffman::from_lengths(&[]).is_err());
        assert!(Huffman::from_lengths(&[1, 1, 1]).is_err());
        assert!(Huffman::from_lengths(&[16]).is_err());
        for seed in 0..128u32 {
            let mut lengths = [0u8; 19];
            for (i, value) in lengths.iter_mut().enumerate() {
                *value = ((seed.wrapping_mul(17).wrapping_add(i as u32 * 11)) % 17) as u8;
            }
            let _ = Huffman::from_lengths(&lengths);
        }
    }
}
