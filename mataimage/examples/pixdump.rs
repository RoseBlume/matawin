use std::env;
use std::fs::File;
use std::io::{Write, BufWriter};
use std::path::Path;

use mataimage::{Image, ImageError};

pub fn load_image_2d<P: AsRef<Path>>(path: P) -> Result<Vec<Vec<u32>>, ImageError> {
    let img = Image::load(path.as_ref().to_str().unwrap())?;

    let width = img.width as usize;
    let height = img.height as usize;

    let mut buffer = vec![vec![0u32; width]; height];

    for y in 0..height {
        for x in 0..width {
            let pixel = img.pixels[y * width + x];
            buffer[y][x] = pixel;
        }
    }

    Ok(buffer)
}

fn write_txt<P: AsRef<Path>>(path: P, buffer: &[Vec<u32>]) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // Find max width for alignment
    let max_width = buffer
        .iter()
        .flat_map(|row| row.iter())
        .map(|v| v.to_string().len())
        .max()
        .unwrap_or(1);

    for row in buffer {
        for value in row {
            write!(writer, "{:width$} ", value, width = max_width)?;
        }
        writeln!(writer)?;
    }

    Ok(())
}

fn write_csv<P: AsRef<Path>>(path: P, buffer: &[Vec<u32>]) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for row in buffer {
        for (i, value) in row.iter().enumerate() {
            if i > 0 {
                write!(writer, ",")?;
            }
            write!(writer, "{}", value)?;
        }
        writeln!(writer)?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Usage: {} <input_image> <output_file> <txt|csv>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let format = &args[3];

    let buffer = load_image_2d(input_path)?;

    match format.as_str() {
        "txt" => {
            write_txt(output_path, &buffer)?;
            println!("Formatted text written to {}", output_path);
        }
        "csv" => {
            write_csv(output_path, &buffer)?;
            println!("CSV written to {}", output_path);
        }
        _ => {
            eprintln!("Invalid format. Use 'txt' or 'csv'");
            std::process::exit(1);
        }
    }

    Ok(())
}