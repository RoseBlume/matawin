//! f32/error, denormal and specialization microbenchmarks for P569/P594/P595.
use std::{hint::black_box,time::Instant};

fn exact_soft(x:f32)->f32{if !x.is_finite(){return 0.0}(x/(1.0+x.abs()*0.15)).clamp(-1.0,0.999_999_94)}
fn approximate_soft(x:f32)->f32{if !x.is_finite(){return 0.0}let a=x.abs();if a>=1.2{return x.signum().clamp(-1.0,0.999_999_94)}let y=0.15*a;(x*(1.0-y+y*y)).clamp(-1.0,0.999_999_94)}

fn main(){
    let mut max_error=0.0f32;for i in 0..=1_000_000{let x=-4.0+8.0*(i as f32/1_000_000.0);max_error=max_error.max((exact_soft(x)-approximate_soft(x)).abs());}
    let normal=[0.25f32;4096];let denormal=[f32::from_bits(1);4096];
    let normal_ns=bench(&normal);let denormal_ns=bench(&denormal);
    println!("soft_limiter_max_abs_error={max_error:.9}");
    println!("documented_fast_math_error_bound=0.006");
    println!("normal_ns={normal_ns}");println!("denormal_ns={denormal_ns}");
    assert!(max_error<=0.006,"fast-math limiter exceeded quantified bound");
}
fn bench(values:&[f32])->u128{let start=Instant::now();let mut sum=0.0f32;for _ in 0..4096{for &v in values{sum=black_box(sum+black_box(v)*1.000_001);}}black_box(sum);start.elapsed().as_nanos()}
