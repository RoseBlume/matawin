//! Microbenchmarks for P598/P599 experiments: stack scratch and setup arenas.
use std::{hint::black_box,time::Instant};

struct Arena{bytes:Vec<u8>,head:usize}
impl Arena{
    fn with_capacity(n:usize)->Self{Self{bytes:vec![0;n],head:0}}
    fn reset(&mut self){self.head=0}
    fn alloc(&mut self,n:usize,align:usize)->Option<&mut[u8]>{let mask=align.max(1).next_power_of_two()-1;let start=(self.head+mask)&!mask;let end=start.checked_add(n)?;if end>self.bytes.len(){return None}self.head=end;Some(&mut self.bytes[start..end])}
}
fn main(){
    const ITERS:usize=100_000;
    let t=Instant::now();let mut sum=0.0f32;for i in 0..ITERS{let mut scratch=[0.0f32;256];scratch[i&255]=i as f32;sum+=scratch[(i*7)&255];black_box(&scratch);}println!("small_stack_ns={},checksum={}",t.elapsed().as_nanos(),sum);
    let t=Instant::now();let mut sum=0.0f32;for i in 0..ITERS{let mut scratch=vec![0.0f32;256];scratch[i&255]=i as f32;sum+=scratch[(i*7)&255];black_box(&scratch);}println!("small_heap_ns={},checksum={}",t.elapsed().as_nanos(),sum);
    let t=Instant::now();let mut arena=Arena::with_capacity(64*1024);let mut checksum=0u64;for i in 0..ITERS{arena.reset();for j in 0..64{let b=arena.alloc(24,8).unwrap();b[0]=(i^j)as u8;checksum+=b[0]as u64;}}println!("setup_arena_ns={},checksum={checksum}",t.elapsed().as_nanos());
    let t=Instant::now();let mut checksum=0u64;for i in 0..ITERS{let mut setup=Vec::with_capacity(64);for j in 0..64{let mut b=vec![0u8;24];b[0]=(i^j)as u8;checksum+=b[0]as u64;setup.push(b);}black_box(setup);}println!("setup_nested_vec_ns={},checksum={checksum}",t.elapsed().as_nanos());
}
