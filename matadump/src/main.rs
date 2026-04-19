mod simd;
use simd::process_ascii;
use std::env;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};

const HEX_TABLE: [[u8; 2]; 256] = {
    let mut table = [[0u8; 2]; 256];
    let mut i = 0;
    while i < 256 {
        let hi = b"0123456789abcdef"[(i >> 4) as usize];
        let lo = b"0123456789abcdef"[(i & 0xf) as usize];
        table[i] = [hi, lo];
        i += 1;
    }
    table
};



fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input> <output>", args[0]);
        std::process::exit(1);
    }

    let input = File::open(&args[1])?;
    let output = File::create(&args[2])?;

    let mut reader = BufReader::with_capacity(256 * 1024, input);
    let mut writer = BufWriter::with_capacity(256 * 1024, output);

    let mut buf = [0u8; 16];
    let mut ascii = [0u8; 16];
    let mut offset = 0usize;

    let mut line = [0u8; 80];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        let mut idx = 0;

        // Offset (manual hex)
        {
            let mut tmp = offset;
            for i in (0..8).rev() {
                line[i] = b"0123456789abcdef"[(tmp & 0xF) as usize];
                tmp >>= 4;
            }
            idx += 8;
        }

        line[idx] = b':'; idx += 1;
        line[idx] = b' '; idx += 1;

        // Hex section (still lookup-based, very fast)
        for i in 0..16 {
            if i < n {
                let h = HEX_TABLE[buf[i] as usize];
                line[idx] = h[0];
                line[idx + 1] = h[1];
                line[idx + 2] = b' ';
            } else {
                line[idx] = b' ';
                line[idx + 1] = b' ';
                line[idx + 2] = b' ';
            }
            idx += 3;
        }

        line[idx] = b' '; idx += 1;
        line[idx] = b'|'; idx += 1;

        // SIMD ASCII conversion
        if n == 16 {
            process_ascii(&buf, &mut ascii);
            line[idx..idx + 16].copy_from_slice(&ascii);
            idx += 16;
        } else {
            // fallback for short reads
            for i in 0..n {
                let b = buf[i];
                line[idx] = if b >= 0x20 && b <= 0x7e { 
                    b 
                } 
                else {
                    b'.' 
                };
                idx += 1;
            }
            for _ in n..16 {
                line[idx] = b' ';
                idx += 1;
            }
        }

        line[idx] = b'|'; idx += 1;
        line[idx] = b'\n'; idx += 1;

        writer.write_all(&line[..idx])?;
        offset += n;
    }

    writer.flush()?;
    Ok(())
}