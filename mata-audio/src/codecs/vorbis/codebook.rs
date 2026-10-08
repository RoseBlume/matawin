use crate::{AudioError, codecs::support::{checked_setup_len, StageTimer}};
use super::bit_reader::{ilog, BitReader};

const FAST_BITS: u8 = 8;
const FAST_SIZE: usize = 1 << FAST_BITS;

#[derive(Debug, Clone)]
struct BuildNode { symbol: Option<u32>, left: Option<Box<BuildNode>>, right: Option<Box<BuildNode>> }
impl BuildNode {
    fn empty()->Self{Self{symbol:None,left:None,right:None}}
    fn full(&self)->bool{self.symbol.is_some()||(self.left.as_deref().is_some_and(BuildNode::full)&&self.right.as_deref().is_some_and(BuildNode::full))}
    fn insert_leftmost(&mut self,depth:u8,symbol:u32)->bool{
        if self.symbol.is_some(){return false;}if depth==0{if self.left.is_some()||self.right.is_some(){return false;}self.symbol=Some(symbol);return true;}
        if self.left.is_none(){self.left=Some(Box::new(Self::empty()));}if let Some(l)=self.left.as_deref_mut(){if !l.full()&&l.insert_leftmost(depth-1,symbol){return true;}}
        if self.right.is_none(){self.right=Some(Box::new(Self::empty()));}if let Some(r)=self.right.as_deref_mut(){if !r.full()&&r.insert_leftmost(depth-1,symbol){return true;}}false
    }
}

#[derive(Debug,Clone,Copy)]
struct FlatNode{symbol:i32,left:i32,right:i32}
#[repr(C)]
#[derive(Debug,Clone,Copy)]
struct FastEntry{symbol:i32,node:i32,secondary:i16,bits:u8,_pad:u8}
impl FastEntry{const EMPTY:Self=Self{symbol:-1,node:-1,secondary:-1,bits:0,_pad:0};}

#[derive(Debug,Clone)]
struct HuffmanTree{
    nodes:Vec<FlatNode>,root:i32,single_symbol:Option<u32>,first:Vec<FastEntry>,second:Vec<FastEntry>,
}
impl HuffmanTree{
    fn from_lengths(lengths:&[u8])->Result<Self,AudioError>{
        let used:Vec<(usize,u8)>=lengths.iter().copied().enumerate().filter(|(_,l)|*l!=0).collect();
        if used.is_empty(){return Ok(Self{nodes:Vec::new(),root:-1,single_symbol:None,first:vec![FastEntry::EMPTY;FAST_SIZE],second:Vec::new()});}
        if used.len()==1{if used[0].1!=1{return Err(AudioError::InvalidFormat("A single-entry Vorbis codebook must use a one-bit codeword".into()));}return Ok(Self{nodes:Vec::new(),root:-1,single_symbol:Some(used[0].0 as u32),first:vec![FastEntry::EMPTY;FAST_SIZE],second:Vec::new()});}
        let mut root=BuildNode::empty();for(symbol,depth)in used{if depth>32||!root.insert_leftmost(depth,symbol as u32){return Err(AudioError::InvalidFormat("Overspecified Vorbis Huffman codebook".into()));}}
        if !root.full(){return Err(AudioError::InvalidFormat("Underpopulated Vorbis Huffman codebook".into()));}
        let mut nodes=Vec::new();let root_index=flatten(&root,&mut nodes);
        let mut tree=Self{nodes,root:root_index,single_symbol:None,first:vec![FastEntry::EMPTY;FAST_SIZE],second:Vec::new()};tree.build_fast_tables();Ok(tree)
    }

    fn build_fast_tables(&mut self){
        use std::collections::BTreeMap;
        let mut unresolved=BTreeMap::<i32,usize>::new();
        for prefix in 0..FAST_SIZE{let entry=self.walk_prefix(self.root,prefix as u32,FAST_BITS);self.first[prefix]=entry;if entry.symbol<0&&entry.node>=0{let next=unresolved.len();let slot=*unresolved.entry(entry.node).or_insert(next);self.first[prefix].secondary=slot as i16;}}
        self.second.resize(unresolved.len()*FAST_SIZE,FastEntry::EMPTY);
        for(node,table)in unresolved{for prefix in 0..FAST_SIZE{self.second[table*FAST_SIZE+prefix]=self.walk_prefix(node,prefix as u32,FAST_BITS);}}
    }

    fn walk_prefix(&self,mut node:i32,prefix:u32,max_bits:u8)->FastEntry{
        for used in 0..max_bits{
            let Some(n)=self.nodes.get(node as usize)else{return FastEntry::EMPTY;};if n.symbol>=0{return FastEntry{symbol:n.symbol,node:-1,secondary:-1,bits:used,_pad:0};}
            let one=(prefix>>used)&1!=0;node=if one{n.right}else{n.left};if node<0{return FastEntry::EMPTY;}
        }
        let n=self.nodes[node as usize];if n.symbol>=0{FastEntry{symbol:n.symbol,node:-1,secondary:-1,bits:max_bits,_pad:0}}else{FastEntry{symbol:-1,node,secondary:-1,bits:max_bits,_pad:0}}
    }

    fn decode(&self,bits:&mut BitReader<'_>)->Result<u32,AudioError>{
        if let Some(symbol)=self.single_symbol{let _=bits.read_bool()?;return Ok(symbol);}
        if bits.remaining_bits()>=FAST_BITS as usize{
            let prefix=bits.peek_bits(FAST_BITS)? as usize;if prefix+16<self.first.len(){crate::simd::prefetch_read(unsafe{self.first.as_ptr().add(prefix+16)});}let first=self.first[prefix];
            if first.symbol>=0{bits.skip_bits(first.bits as usize)?;return Ok(first.symbol as u32);}
            if first.node>=0{bits.skip_bits(FAST_BITS as usize)?;if bits.remaining_bits()>=FAST_BITS as usize&&first.secondary>=0{let p2=bits.peek_bits(FAST_BITS)? as usize;let second=self.second[first.secondary as usize*FAST_SIZE+p2];if second.symbol>=0{bits.skip_bits(second.bits as usize)?;return Ok(second.symbol as u32);}bits.skip_bits(FAST_BITS as usize)?;return self.decode_from_node(bits,second.node);}return self.decode_from_node(bits,first.node);}
        }
        self.decode_from_node(bits,self.root)
    }

    fn decode_from_node(&self,bits:&mut BitReader<'_>,mut node:i32)->Result<u32,AudioError>{
        loop{let n=*self.nodes.get(node as usize).ok_or_else(||AudioError::InvalidFormat("Invalid Vorbis Huffman node".into()))?;if n.symbol>=0{return Ok(n.symbol as u32);}node=if bits.read_bool()?{n.right}else{n.left};if node<0{return Err(AudioError::InvalidFormat("Invalid Vorbis Huffman codeword".into()));}}
    }
}
fn flatten(node:&BuildNode,out:&mut Vec<FlatNode>)->i32{let index=out.len()as i32;out.push(FlatNode{symbol:node.symbol.map_or(-1,|s|s as i32),left:-1,right:-1});let left=node.left.as_deref().map(|n|flatten(n,out)).unwrap_or(-1);let right=node.right.as_deref().map(|n|flatten(n,out)).unwrap_or(-1);out[index as usize].left=left;out[index as usize].right=right;index}

#[derive(Debug,Clone)]
pub(super) struct Codebook{pub dimensions:u16,pub entries:u32,tree:HuffmanTree,lookup:Option<Box<[f32]>>}
impl Codebook{
    #[inline]pub fn decode_scalar(&self,bits:&mut BitReader<'_>)->Result<u32,AudioError>{let _stage_timer=StageTimer::start("vorbis.codebook");self.tree.decode(bits)}
    pub fn decode_vector<'a>(&'a self,bits:&mut BitReader<'_>)->Result<&'a[f32],AudioError>{let symbol=self.decode_scalar(bits)?as usize;if symbol>=self.entries as usize{return Err(AudioError::InvalidFormat("Vorbis codebook produced an out-of-range entry".into()));}let lookup=self.lookup.as_ref().ok_or_else(||AudioError::InvalidFormat("Vorbis codebook has no VQ lookup table".into()))?;let dimensions=self.dimensions as usize;let start=symbol.checked_mul(dimensions).ok_or_else(||AudioError::InvalidFormat("Vorbis codebook lookup offset overflow".into()))?;if start+dimensions+32<lookup.len(){crate::simd::prefetch_read(unsafe{lookup.as_ptr().add(start+dimensions+32)});}lookup.get(start..start+dimensions).ok_or_else(||AudioError::InvalidFormat("Vorbis codebook lookup is truncated".into()))}
    #[inline]pub fn has_lookup(&self)->bool{self.lookup.is_some()}
    pub(super) fn retained_bytes(&self)->usize{
        self.tree.nodes.capacity()*core::mem::size_of::<FlatNode>()
            +self.tree.first.capacity()*core::mem::size_of::<FastEntry>()
            +self.tree.second.capacity()*core::mem::size_of::<FastEntry>()
            +self.lookup.as_ref().map_or(0,|v|v.len()*core::mem::size_of::<f32>())
    }
}

pub(super) fn read_codebook(bits:&mut BitReader<'_>)->Result<Codebook,AudioError>{
    if bits.read_u32(24)?!=0x56_43_42{return Err(AudioError::InvalidFormat("Invalid Vorbis codebook sync pattern".into()));}
    let dimensions=bits.read_u16(16)?as usize;let entries=bits.read_u32(24)?as usize;if dimensions==0{return Err(AudioError::InvalidFormat("Vorbis codebook has zero dimensions".into()));}checked_setup_len::<u8>(entries,"Vorbis codebook lengths")?;
    let ordered=bits.read_bool()?;let mut lengths=Vec::with_capacity(entries);
    if ordered{let mut current_entry=0usize;let mut current_length=bits.read_u8(5)?+1;while current_entry<entries{let count_bits=ilog((entries-current_entry)as u64);let count=bits.read_u32(count_bits)?as usize;if current_entry.saturating_add(count)>entries{return Err(AudioError::InvalidFormat("Invalid ordered Vorbis codebook lengths".into()));}lengths.extend(std::iter::repeat_n(current_length,count));current_entry+=count;if current_entry<entries{current_length=current_length.checked_add(1).ok_or_else(||AudioError::InvalidFormat("Vorbis codeword length overflow".into()))?;if current_length>32{return Err(AudioError::InvalidFormat("Vorbis codeword length exceeds 32 bits".into()));}}}}
    else{let sparse=bits.read_bool()?;for _ in 0..entries{if sparse&&!bits.read_bool()?{lengths.push(0)}else{lengths.push(bits.read_u8(5)?+1)}}}
    let tree=HuffmanTree::from_lengths(&lengths)?;let lookup_type=bits.read_u8(4)?;if lookup_type>2{return Err(AudioError::InvalidFormat(format!("Unsupported Vorbis codebook lookup type {lookup_type}")));}
    let lookup=if lookup_type==0{None}else{let minimum=bits.read_vorbis_f32()?;let delta=bits.read_vorbis_f32()?;let value_bits=bits.read_u8(4)?+1;let sequence=bits.read_bool()?;let lookup_values=if lookup_type==1{lookup1_values(entries,dimensions)?}else{entries.checked_mul(dimensions).ok_or_else(||AudioError::InvalidFormat("Vorbis codebook lookup size overflow".into()))?};checked_setup_len::<u32>(lookup_values,"Vorbis multiplicands")?;let mut multiplicands=Vec::with_capacity(lookup_values);for _ in 0..lookup_values{multiplicands.push(bits.read_u32(value_bits)?);}let total=entries.checked_mul(dimensions).ok_or_else(||AudioError::InvalidFormat("Vorbis codebook decoded lookup size overflow".into()))?;checked_setup_len::<f32>(total,"Vorbis codebook lookup")?;let mut decoded=Vec::with_capacity(total);for entry in 0..entries{let mut last=0.0f32;let mut divisor=1usize;for dim in 0..dimensions{let index=if lookup_type==1{let idx=(entry/divisor)%lookup_values;divisor=divisor.checked_mul(lookup_values).unwrap_or(usize::MAX);idx}else{entry*dimensions+dim};let mult=*multiplicands.get(index).ok_or_else(||AudioError::InvalidFormat("Vorbis codebook multiplicand index overflow".into()))?as f32;let value=mult*delta+minimum+if sequence{last}else{0.0};decoded.push(value);if sequence{last=value;}}}Some(decoded.into_boxed_slice())};
    Ok(Codebook{dimensions:dimensions as u16,entries:entries as u32,tree,lookup})
}
fn lookup1_values(entries:usize,dimensions:usize)->Result<usize,AudioError>{if entries==0{return Ok(0);}if dimensions==0{return Err(AudioError::InvalidFormat("Vorbis lookup1 codebook has zero dimensions".into()));}let(mut low,mut high)=(1usize,entries.saturating_add(1));while low+1<high{let mid=low+(high-low)/2;if power_leq(mid,dimensions,entries){low=mid}else{high=mid}}Ok(low)}
fn power_leq(base:usize,exponent:usize,limit:usize)->bool{let mut value=1usize;for _ in 0..exponent{match value.checked_mul(base){Some(next)if next<=limit=>value=next,_=>return false}}true}
