//! Convert headerless f32 embeddings into the versioned `SCQ8` format.
//!
//! Usage:
//! `quantize_embeddings <in.bin> <in-ids.bin> <out.bin> <out-ids.bin> <dim>`

use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use rhema_detection::semantic::quantize::{
    dequantize_vector, encode_header, quantize_vector, sha256,
};

fn invalid_input(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    let dot: f32 = left.iter().zip(right).map(|(a, b)| a * b).sum();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        1.0
    } else {
        dot / (left_norm * right_norm)
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "asset byte counts are far below f64's exact integer range"
)]
fn mebibytes(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

fn convert(
    input: &Path,
    input_ids: &Path,
    output: &Path,
    output_ids: &Path,
    dim: usize,
) -> Result<(), Box<dyn Error>> {
    if dim == 0 {
        return Err(invalid_input("dim must be greater than zero").into());
    }
    let vector_byte_len = dim
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| invalid_input("vector byte length overflows usize"))?;
    let input_byte_len = usize::try_from(std::fs::metadata(input)?.len())
        .map_err(|_| invalid_input("input embeddings file is larger than usize"))?;
    if input_byte_len % vector_byte_len != 0 {
        return Err(invalid_input(format!(
            "input length {input_byte_len} is not a multiple of {vector_byte_len}"
        ))
        .into());
    }
    let num_vectors = input_byte_len / vector_byte_len;
    if num_vectors == 0 {
        return Err(invalid_input("input embeddings file contains no vectors").into());
    }

    let ids_bytes = std::fs::read(input_ids)?;
    let expected_ids_len = num_vectors
        .checked_mul(std::mem::size_of::<i64>())
        .ok_or_else(|| invalid_input("IDs byte length overflows usize"))?;
    if ids_bytes.len() != expected_ids_len {
        return Err(invalid_input(format!(
            "vector count mismatch: {num_vectors} embeddings vs {} IDs",
            ids_bytes.len() / std::mem::size_of::<i64>()
        ))
        .into());
    }

    let mut reader = BufReader::new(File::open(input)?);
    let mut vector_bytes = vec![0u8; vector_byte_len];
    let mut scales = Vec::with_capacity(num_vectors);
    let data_capacity = num_vectors
        .checked_mul(dim)
        .ok_or_else(|| invalid_input("quantized data length overflows usize"))?;
    let mut data = Vec::with_capacity(data_capacity);
    let mut worst_cosine = 1.0f32;

    for vector_index in 0..num_vectors {
        reader.read_exact(&mut vector_bytes)?;
        let vector: Vec<f32> = vector_bytes
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|bytes| {
                f32::from_le_bytes(
                    bytes
                        .try_into()
                        .expect("chunks_exact guarantees four component bytes"),
                )
            })
            .collect();
        let (quantized, scale) = quantize_vector(&vector).map_err(|error| {
            invalid_input(format!(
                "vector {vector_index} cannot be quantized: {error}"
            ))
        })?;
        worst_cosine = worst_cosine.min(cosine(&vector, &dequantize_vector(&quantized, scale)));
        scales.push(scale);
        data.extend(
            quantized
                .into_iter()
                .map(|component| component.to_ne_bytes()[0]),
        );
    }

    let header = encode_header(dim, num_vectors, sha256(&ids_bytes))?;
    let mut writer = BufWriter::new(File::create(output)?);
    writer.write_all(&header)?;
    for scale in scales {
        writer.write_all(&scale.to_le_bytes())?;
    }
    writer.write_all(&data)?;
    writer.flush()?;
    std::fs::write(output_ids, ids_bytes)?;

    let output_byte_len = std::fs::metadata(output)?.len();
    println!("vectors={num_vectors} dim={dim}");
    println!("worst per-vector cosine={worst_cosine:.6}");
    println!(
        "size={:.2} MiB -> {:.2} MiB",
        mebibytes(u64::try_from(input_byte_len)?),
        mebibytes(output_byte_len)
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, input_ids, output, output_ids, dim] = args.as_slice() else {
        return Err(invalid_input(
            "usage: quantize_embeddings <in.bin> <in-ids.bin> <out.bin> <out-ids.bin> <dim>",
        )
        .into());
    };
    let dim = dim
        .parse::<usize>()
        .map_err(|error| invalid_input(format!("dim must be a positive integer: {error}")))?;
    convert(
        Path::new(input),
        Path::new(input_ids),
        Path::new(output),
        Path::new(output_ids),
        dim,
    )
}

fn main() {
    if let Err(error) = run() {
        eprintln!("quantize_embeddings: {error}");
        std::process::exit(1);
    }
}
