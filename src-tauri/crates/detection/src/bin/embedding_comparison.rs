//! Compare f32 and q8 retrieval quality and timing on real corpus vectors.

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rhema_detection::semantic::index::VectorIndex;
use rhema_detection::HnswVectorIndex;

fn invalid_input(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn required_path(args: &[String], name: &str) -> Result<PathBuf, Box<dyn Error>> {
    arg(args, name)
        .map(PathBuf::from)
        .ok_or_else(|| invalid_input(format!("missing required argument {name}")).into())
}

fn parse_or<T>(args: &[String], name: &str, default: T) -> Result<T, Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    arg(args, name).map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|error| invalid_input(format!("invalid {name}: {error}")).into())
    })
}

fn read_query(
    file: &mut File,
    vector_index: usize,
    dim: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let vector_byte_len = dim
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| invalid_input("query vector byte length overflows usize"))?;
    let offset = vector_index
        .checked_mul(vector_byte_len)
        .ok_or_else(|| invalid_input("query offset overflows usize"))?;
    file.seek(SeekFrom::Start(u64::try_from(offset)?))?;
    let mut bytes = vec![0u8; vector_byte_len];
    file.read_exact(&mut bytes)?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|component| {
            f32::from_le_bytes(
                component
                    .try_into()
                    .expect("chunks_exact guarantees four component bytes"),
            )
        })
        .collect())
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the percentile is bounded to 0..=1 and benchmark sample counts are small"
)]
fn percentile(samples: &mut [Duration], percentile: f64) -> Duration {
    samples.sort_unstable();
    let last = samples.len().saturating_sub(1);
    let index = ((last as f64) * percentile).round() as usize;
    samples[index.min(last)]
}

#[expect(
    clippy::too_many_lines,
    reason = "diagnostic binary keeps its paired measurement flow together"
)]
#[expect(
    clippy::cast_precision_loss,
    reason = "bounded benchmark counts fit exactly enough for reporting ratios"
)]
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let f32_path = required_path(&args, "--f32")?;
    let f32_ids_path = required_path(&args, "--f32-ids")?;
    let q8_path = required_path(&args, "--q8")?;
    let q8_ids_path = required_path(&args, "--q8-ids")?;
    let dim: usize = parse_or(&args, "--dim", 384)?;
    let query_count: usize = parse_or(&args, "--queries", 256)?;
    let k: usize = parse_or(&args, "--k", 10)?;
    let min_top1: f64 = parse_or(&args, "--min-top1", 0.995)?;
    let min_overlap: f64 = parse_or(&args, "--min-overlap", 0.99)?;
    let max_similarity_drift: f64 = parse_or(&args, "--max-similarity-drift", 0.01)?;
    if dim == 0 || query_count == 0 || k == 0 {
        return Err(invalid_input("dim, queries, and k must be greater than zero").into());
    }

    let f32_load_started = Instant::now();
    let f32_index = HnswVectorIndex::load(&f32_path, &f32_ids_path, dim)?;
    let f32_load = f32_load_started.elapsed();
    let q8_load_started = Instant::now();
    let q8_index = HnswVectorIndex::load(&q8_path, &q8_ids_path, dim)?;
    let q8_load = q8_load_started.elapsed();
    if f32_index.len() != q8_index.len() {
        return Err(invalid_input(format!(
            "index length mismatch: f32={} q8={}",
            f32_index.len(),
            q8_index.len()
        ))
        .into());
    }

    let sample_count = query_count.min(f32_index.len());
    let mut query_file = File::open(&f32_path)?;
    let mut top1_matches = 0usize;
    let mut exact_order_matches = 0usize;
    let mut overlap_total = 0usize;
    let mut comparable_total = 0usize;
    let mut max_drift = 0.0f64;
    let mut f32_times = Vec::with_capacity(sample_count);
    let mut q8_times = Vec::with_capacity(sample_count);

    for sample in 0..sample_count {
        let vector_index = if sample_count == 1 {
            0
        } else {
            sample * (f32_index.len() - 1) / (sample_count - 1)
        };
        let query = read_query(&mut query_file, vector_index, dim)?;

        let (f32_results, q8_results) = if sample % 2 == 0 {
            let started = Instant::now();
            let f32_results = f32_index.search(&query, k)?;
            f32_times.push(started.elapsed());
            let started = Instant::now();
            let q8_results = q8_index.search(&query, k)?;
            q8_times.push(started.elapsed());
            (f32_results, q8_results)
        } else {
            let started = Instant::now();
            let q8_results = q8_index.search(&query, k)?;
            q8_times.push(started.elapsed());
            let started = Instant::now();
            let f32_results = f32_index.search(&query, k)?;
            f32_times.push(started.elapsed());
            (f32_results, q8_results)
        };

        if f32_results.first().map(|result| result.verse_id)
            == q8_results.first().map(|result| result.verse_id)
        {
            top1_matches += 1;
        }
        if f32_results
            .iter()
            .map(|result| result.verse_id)
            .eq(q8_results.iter().map(|result| result.verse_id))
        {
            exact_order_matches += 1;
        }

        let mut q8_counts: HashMap<i64, usize> = HashMap::new();
        for result in &q8_results {
            *q8_counts.entry(result.verse_id).or_default() += 1;
        }
        for result in &f32_results {
            if let Some(remaining) = q8_counts.get_mut(&result.verse_id) {
                if *remaining > 0 {
                    overlap_total += 1;
                    *remaining -= 1;
                }
            }
            comparable_total += 1;
        }
        for (f32_result, q8_result) in f32_results.iter().zip(&q8_results) {
            if f32_result.verse_id == q8_result.verse_id {
                max_drift = max_drift.max((f32_result.similarity - q8_result.similarity).abs());
            }
        }
    }

    let top1_agreement = top1_matches as f64 / sample_count as f64;
    let exact_order_agreement = exact_order_matches as f64 / sample_count as f64;
    let topk_overlap = overlap_total as f64 / comparable_total as f64;
    let f32_p50 = percentile(&mut f32_times, 0.50);
    let f32_p95 = percentile(&mut f32_times, 0.95);
    let q8_p50 = percentile(&mut q8_times, 0.50);
    let q8_p95 = percentile(&mut q8_times, 0.95);

    println!(
        "vectors={} dim={dim} queries={sample_count} k={k}",
        f32_index.len()
    );
    println!(
        "load_ms f32={:.3} q8={:.3}",
        f32_load.as_secs_f64() * 1_000.0,
        q8_load.as_secs_f64() * 1_000.0
    );
    println!(
        "search_ms f32_p50={:.3} f32_p95={:.3} q8_p50={:.3} q8_p95={:.3}",
        f32_p50.as_secs_f64() * 1_000.0,
        f32_p95.as_secs_f64() * 1_000.0,
        q8_p50.as_secs_f64() * 1_000.0,
        q8_p95.as_secs_f64() * 1_000.0
    );
    println!(
        "quality top1_agreement={top1_agreement:.6} exact_order_agreement={exact_order_agreement:.6} topk_overlap={topk_overlap:.6} max_similarity_drift={max_drift:.6}"
    );

    if top1_agreement < min_top1 || topk_overlap < min_overlap || max_drift > max_similarity_drift {
        return Err(invalid_input(format!(
            "comparison gate failed: top1={top1_agreement:.6}/{min_top1:.6} overlap={topk_overlap:.6}/{min_overlap:.6} drift={max_drift:.6}/{max_similarity_drift:.6}"
        ))
        .into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("embedding_comparison: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::percentile;

    #[test]
    fn percentile_returns_requested_order_statistic() {
        let mut samples = [
            Duration::from_millis(30),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ];
        assert_eq!(percentile(&mut samples, 0.50), Duration::from_millis(20));
    }
}
