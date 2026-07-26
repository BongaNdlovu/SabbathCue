//! Brute-force cosine-similarity vector index.
//!
//! The scan is exhaustive, but parallel and multi-accumulator, which keeps a
//! ~62 K x 384 corpus in the single-digit milliseconds — still cheap enough
//! that approximate nearest-neighbour structures are not worth their recall
//! cost.
//!
//! This module is only compiled when the `vector-search` feature is enabled.

#[cfg(feature = "vector-search")]
use std::io::{Read, Seek};
#[cfg(feature = "vector-search")]
use std::path::Path;

#[cfg(feature = "vector-search")]
use rayon::prelude::*;

#[cfg(feature = "vector-search")]
use super::index::{SearchResult, VectorIndex};
#[cfg(feature = "vector-search")]
use crate::error::DetectionError;

/// Vector index backed by a flat array of pre-computed embeddings.
///
/// Search is exhaustive (brute-force dot product).  Because all stored
/// vectors and query vectors are L2-normalised, the dot product equals
/// cosine similarity.
#[cfg(feature = "vector-search")]
#[derive(Debug)]
pub struct HnswVectorIndex {
    vectors: Vectors,
    /// Verse (or row) identifiers, one per stored vector.
    verse_ids: Vec<i64>,
    /// Dimensionality of each embedding vector.
    dimension: usize,
}

#[cfg(feature = "vector-search")]
#[derive(Debug)]
enum Vectors {
    F32(Vec<f32>),
    Q8 { data: Vec<i8>, scales: Vec<f32> },
}

#[cfg(feature = "vector-search")]
impl HnswVectorIndex {
    /// Load pre-computed embeddings and their verse IDs from binary files.
    ///
    /// **Embeddings file** — a sequence of `f32` values in native byte
    /// order.  Each consecutive `dim` floats form one vector.
    ///
    /// **IDs file** — a sequence of `i64` values in native byte order,
    /// one per vector.
    pub fn load(
        embeddings_path: &Path,
        ids_path: &Path,
        dim: usize,
    ) -> Result<Self, DetectionError> {
        if dim == 0 {
            return Err(DetectionError::Internal(
                "embedding dimension must be greater than 0".into(),
            ));
        }

        // --- Read embeddings ---
        // Read straight into the f32 buffer. The file is a flat little-endian
        // f32 array, so on little-endian targets the bytes need no decoding;
        // going via an intermediate byte Vec would double peak memory (~91 MB
        // each for the shipped corpus) and cost a pass over ~24 M elements.
        let mut file = std::fs::File::open(embeddings_path).map_err(|e| {
            DetectionError::Internal(format!(
                "read embeddings {}: {e}",
                embeddings_path.display()
            ))
        })?;

        let byte_len = file
            .metadata()
            .map_err(|e| {
                DetectionError::Internal(format!(
                    "stat embeddings {}: {e}",
                    embeddings_path.display()
                ))
            })
            .and_then(|meta| {
                usize::try_from(meta.len()).map_err(|_| {
                    DetectionError::Internal("embeddings file is larger than usize".into())
                })
            })?;

        let prefix_len = byte_len.min(super::quantize::Q8_HEADER_LEN);
        let mut header_buf = [0u8; super::quantize::Q8_HEADER_LEN];
        file.read_exact(&mut header_buf[..prefix_len])
            .map_err(|e| {
                DetectionError::Internal(format!(
                    "read embeddings header {}: {e}",
                    embeddings_path.display()
                ))
            })?;
        let q8_header = super::quantize::decode_header(&header_buf[..prefix_len])
            .map_err(|e| DetectionError::Internal(format!("invalid q8 embeddings: {e}")))?;

        if let Some(header) = q8_header {
            return Self::load_q8(&mut file, embeddings_path, ids_path, dim, byte_len, header);
        }
        Self::load_f32(&mut file, embeddings_path, ids_path, dim, byte_len)
    }

    fn load_f32(
        file: &mut std::fs::File,
        embeddings_path: &Path,
        ids_path: &Path,
        dim: usize,
        byte_len: usize,
    ) -> Result<Self, DetectionError> {
        file.rewind().map_err(|e| {
            DetectionError::Internal(format!(
                "rewind embeddings {}: {e}",
                embeddings_path.display()
            ))
        })?;
        if !byte_len.is_multiple_of(std::mem::size_of::<f32>()) {
            return Err(DetectionError::Internal(
                "embeddings file size is not a multiple of 4".into(),
            ));
        }
        let float_count = byte_len / std::mem::size_of::<f32>();
        if !float_count.is_multiple_of(dim) {
            return Err(DetectionError::Internal(format!(
                "embeddings length {float_count} is not a multiple of dim {dim}"
            )));
        }
        let num_vectors = float_count / dim;

        let mut embeddings = vec![0f32; float_count];
        file.read_exact(bytemuck::cast_slice_mut(&mut embeddings))
            .map_err(|e| {
                DetectionError::Internal(format!(
                    "read embeddings {}: {e}",
                    embeddings_path.display()
                ))
            })?;

        let index = Self::finish_load(Vectors::F32(embeddings), ids_path, dim, num_vectors, None)?;
        log::info!("HnswVectorIndex loaded: {num_vectors} vectors, dim={dim}");
        Ok(index)
    }

    fn load_q8(
        file: &mut std::fs::File,
        embeddings_path: &Path,
        ids_path: &Path,
        dim: usize,
        byte_len: usize,
        header: super::quantize::Q8Header,
    ) -> Result<Self, DetectionError> {
        if header.dim != dim {
            return Err(DetectionError::Internal(format!(
                "quantized embeddings dim {} != expected dim {dim}",
                header.dim
            )));
        }
        let scale_byte_len = header
            .num_vectors
            .checked_mul(std::mem::size_of::<f32>())
            .ok_or_else(|| {
                DetectionError::Internal("quantized scale length overflows usize".into())
            })?;
        let data_byte_len = header.num_vectors.checked_mul(dim).ok_or_else(|| {
            DetectionError::Internal("quantized vector length overflows usize".into())
        })?;
        let expected_byte_len = super::quantize::Q8_HEADER_LEN
            .checked_add(scale_byte_len)
            .and_then(|length| length.checked_add(data_byte_len))
            .ok_or_else(|| {
                DetectionError::Internal("quantized total length overflows usize".into())
            })?;
        if byte_len != expected_byte_len {
            return Err(DetectionError::Internal(format!(
                "quantized embeddings length {byte_len} != expected {expected_byte_len}"
            )));
        }

        let mut scale_bytes = vec![0u8; scale_byte_len];
        file.read_exact(&mut scale_bytes).map_err(|e| {
            DetectionError::Internal(format!("read q8 scales {}: {e}", embeddings_path.display()))
        })?;
        let scales = Self::decode_scales(&scale_bytes)?;
        let mut data = vec![0i8; data_byte_len];
        file.read_exact(bytemuck::cast_slice_mut(&mut data))
            .map_err(|e| {
                DetectionError::Internal(format!(
                    "read q8 vectors {}: {e}",
                    embeddings_path.display()
                ))
            })?;

        let index = Self::finish_load(
            Vectors::Q8 { data, scales },
            ids_path,
            dim,
            header.num_vectors,
            Some(header.ids_sha256),
        )?;
        log::info!(
            "HnswVectorIndex loaded (int8): {} vectors, dim={dim}",
            header.num_vectors
        );
        Ok(index)
    }

    fn decode_scales(bytes: &[u8]) -> Result<Vec<f32>, DetectionError> {
        bytes
            .chunks_exact(std::mem::size_of::<f32>())
            .enumerate()
            .map(|(index, bytes)| {
                let scale = f32::from_le_bytes(
                    bytes
                        .try_into()
                        .expect("chunks_exact guarantees four scale bytes"),
                );
                if scale.is_finite() && scale >= 0.0 {
                    Ok(scale)
                } else {
                    Err(DetectionError::Internal(format!(
                        "quantized embeddings scale {index} is invalid"
                    )))
                }
            })
            .collect()
    }

    /// Build an index directly from in-memory data.
    ///
    /// Useful for tests or when embeddings have just been computed.
    pub fn from_vecs(
        embeddings: Vec<Vec<f32>>,
        verse_ids: Vec<i64>,
        dim: usize,
    ) -> Result<Self, DetectionError> {
        if embeddings.len() != verse_ids.len() {
            return Err(DetectionError::Internal(
                "embeddings and verse_ids length mismatch".into(),
            ));
        }
        if dim == 0 {
            return Err(DetectionError::Internal(
                "embedding dimension must be greater than 0".into(),
            ));
        }
        if let Some((index, embedding)) = embeddings
            .iter()
            .enumerate()
            .find(|(_, embedding)| embedding.len() != dim)
        {
            return Err(DetectionError::Internal(format!(
                "embedding {index} dim {} != expected dim {dim}",
                embedding.len()
            )));
        }

        let flat: Vec<f32> = embeddings.into_iter().flatten().collect();

        Ok(Self {
            vectors: Vectors::F32(flat),
            verse_ids,
            dimension: dim,
        })
    }

    fn finish_load(
        vectors: Vectors,
        ids_path: &Path,
        dim: usize,
        num_vectors: usize,
        expected_ids_sha256: Option<[u8; 32]>,
    ) -> Result<Self, DetectionError> {
        let ids_bytes = std::fs::read(ids_path).map_err(|e| {
            DetectionError::Internal(format!("read ids {}: {e}", ids_path.display()))
        })?;
        if expected_ids_sha256
            .is_some_and(|expected| super::quantize::sha256(&ids_bytes) != expected)
        {
            return Err(DetectionError::Internal(format!(
                "quantized embeddings IDs digest does not match {}",
                ids_path.display()
            )));
        }
        if ids_bytes.len() % std::mem::size_of::<i64>() != 0 {
            return Err(DetectionError::Internal(
                "ids file size is not a multiple of 8".into(),
            ));
        }

        let verse_ids: Vec<i64> = ids_bytes
            .chunks_exact(std::mem::size_of::<i64>())
            .map(|chunk| i64::from_le_bytes(chunk.try_into().expect("chunk length is 8")))
            .collect();
        if verse_ids.len() != num_vectors {
            return Err(DetectionError::Internal(format!(
                "vector count mismatch: {} embeddings vs {} ids",
                num_vectors,
                verse_ids.len()
            )));
        }

        Ok(Self {
            vectors,
            verse_ids,
            dimension: dim,
        })
    }
}

#[cfg(feature = "vector-search")]
impl VectorIndex for HnswVectorIndex {
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, DetectionError> {
        if query.len() != self.dimension {
            return Err(DetectionError::Internal(format!(
                "query dim {} != index dim {}",
                query.len(),
                self.dimension
            )));
        }

        let n = self.verse_ids.len();
        if n == 0 {
            return Ok(vec![]);
        }

        let k = k.min(n);
        if k == 0 {
            return Ok(vec![]);
        }

        // Dot product (= cosine similarity for L2-normalised vectors) against
        // every stored vector, split across cores.
        let mut scores: Vec<(usize, f32)> = match &self.vectors {
            Vectors::F32(embeddings) => embeddings
                .par_chunks_exact(self.dimension)
                .enumerate()
                .map(|(index, stored)| (index, dot(query, stored)))
                .collect(),
            Vectors::Q8 { data, scales } => data
                .par_chunks_exact(self.dimension)
                .enumerate()
                .map(|(index, stored)| (index, dot_q8(query, stored, scales[index])))
                .collect(),
        };

        // Only k results are returned, so select them in O(n) rather than
        // sorting all n, then order just the survivors.
        scores.select_nth_unstable_by(k - 1, rank);
        scores.truncate(k);
        scores.sort_unstable_by(rank);

        let results: Vec<SearchResult> = scores
            .into_iter()
            .map(|(index, similarity)| SearchResult {
                verse_id: self.verse_ids[index],
                similarity: f64::from(similarity),
            })
            .collect();

        Ok(results)
    }

    fn len(&self) -> usize {
        self.verse_ids.len()
    }
}

/// Independent accumulators in the dot product.
///
/// A plain `.sum()` over the products is one serial float-add dependency
/// chain, and the compiler is not allowed to reassociate it (float addition is
/// not associative), so it cannot vectorise. Separate lanes give it
/// independent chains to keep in flight.
#[cfg(feature = "vector-search")]
const DOT_LANES: usize = 8;

#[cfg(feature = "vector-search")]
#[inline]
fn dot(query: &[f32], stored: &[f32]) -> f32 {
    let mut lanes = [0f32; DOT_LANES];
    let mut query_chunks = query.chunks_exact(DOT_LANES);
    let mut stored_chunks = stored.chunks_exact(DOT_LANES);

    for (q, s) in query_chunks.by_ref().zip(stored_chunks.by_ref()) {
        for ((lane, a), b) in lanes.iter_mut().zip(q).zip(s) {
            *lane += a * b;
        }
    }

    let tail: f32 = query_chunks
        .remainder()
        .iter()
        .zip(stored_chunks.remainder())
        .map(|(a, b)| a * b)
        .sum();

    lanes.iter().sum::<f32>() + tail
}

#[cfg(feature = "vector-search")]
#[inline]
fn dot_q8(query: &[f32], stored: &[i8], scale: f32) -> f32 {
    let mut lanes = [0f32; DOT_LANES];
    let mut query_chunks = query.chunks_exact(DOT_LANES);
    let mut stored_chunks = stored.chunks_exact(DOT_LANES);

    for (query_chunk, stored_chunk) in query_chunks.by_ref().zip(stored_chunks.by_ref()) {
        for ((lane, query_component), stored_component) in
            lanes.iter_mut().zip(query_chunk).zip(stored_chunk)
        {
            *lane += *query_component * f32::from(*stored_component);
        }
    }
    let tail: f32 = query_chunks
        .remainder()
        .iter()
        .zip(stored_chunks.remainder())
        .map(|(query_component, stored_component)| *query_component * f32::from(*stored_component))
        .sum();
    (lanes.iter().sum::<f32>() + tail) * scale
}

/// Rank by descending similarity, breaking ties by index so the top-k order is
/// deterministic — the previous full unstable sort left tied scores in an
/// arbitrary order.
#[cfg(feature = "vector-search")]
fn rank(a: &(usize, f32), b: &(usize, f32)) -> std::cmp::Ordering {
    b.1.partial_cmp(&a.1)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.0.cmp(&b.0))
}

#[cfg(all(test, feature = "vector-search"))]
mod tests {
    use std::io::Write;

    use super::*;

    fn make_unit_vec(dim: usize, hot: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; dim];
        v[hot] = 1.0;
        v
    }

    fn write_ids(path: &Path, ids: &[i64]) -> Vec<u8> {
        let bytes: Vec<u8> = ids.iter().flat_map(|id| id.to_le_bytes()).collect();
        std::fs::write(path, &bytes).expect("write ids");
        bytes
    }

    fn write_q8(path: &Path, vectors: &[Vec<f32>], ids_bytes: &[u8], dim: usize) {
        let mut file = std::fs::File::create(path).expect("create q8");
        file.write_all(
            &super::super::quantize::encode_header(
                dim,
                vectors.len(),
                super::super::quantize::sha256(ids_bytes),
            )
            .expect("encode header"),
        )
        .expect("write header");

        let quantized: Vec<(Vec<i8>, f32)> = vectors
            .iter()
            .map(|vector| super::super::quantize::quantize_vector(vector).expect("finite vector"))
            .collect();
        for (_, scale) in &quantized {
            file.write_all(&scale.to_le_bytes()).expect("write scale");
        }
        for (data, _) in &quantized {
            file.write_all(bytemuck::cast_slice(data))
                .expect("write vector");
        }
    }

    #[test]
    fn test_brute_force_search() {
        let dim = 4;
        let embeddings = vec![
            make_unit_vec(dim, 0), // id 10
            make_unit_vec(dim, 1), // id 20
            make_unit_vec(dim, 2), // id 30
        ];
        let ids = vec![10, 20, 30];

        let index = HnswVectorIndex::from_vecs(embeddings, ids, dim).unwrap();
        assert_eq!(index.len(), 3);

        // Query closest to the second vector
        let query = make_unit_vec(dim, 1);
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].verse_id, 20);
        assert!((results[0].similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_empty_index() {
        let index = HnswVectorIndex::from_vecs(vec![], vec![], 4).unwrap();
        assert!(index.is_empty());
        let results = index.search(&[0.0; 4], 5).unwrap();
        assert!(results.is_empty());
    }

    /// Pins the optimised scan to the original one: the parallel multi-lane
    /// dot product plus partial selection must return the same verses in the
    /// same order as a serial `.sum()` over a full sort. Similarities may
    /// differ only by f32 summation-order rounding.
    #[test]
    fn matches_reference_serial_scan() {
        fn pseudo_random(state: &mut u64) -> f32 {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let bits = u16::try_from(*state >> 48).expect("shifted to 16 bits");
            f32::from(bits) / 32768.0 - 1.0
        }

        let dim = 384;
        let mut state = 0x5eed_u64;
        let mut vectors: Vec<Vec<f32>> = Vec::new();
        for _ in 0..500 {
            let mut v: Vec<f32> = (0..dim).map(|_| pseudo_random(&mut state)).collect();
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            for x in &mut v {
                *x /= norm;
            }
            vectors.push(v);
        }
        let ids: Vec<i64> = (0..500_i64).collect();
        let query = vectors[7].clone();

        // Reference: the pre-optimisation implementation.
        let mut reference: Vec<(usize, f32)> = vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, query.iter().zip(v.iter()).map(|(a, b)| a * b).sum()))
            .collect();
        reference.sort_by(|a, b| b.1.partial_cmp(&a.1).expect("finite similarities"));

        let index = HnswVectorIndex::from_vecs(vectors, ids, dim).unwrap();
        let results = index.search(&query, 12).unwrap();

        assert_eq!(results.len(), 12);
        // The query is vectors[7], so it must be its own nearest neighbour.
        assert_eq!(results[0].verse_id, 7);
        for (result, (expected_index, expected_similarity)) in results.iter().zip(&reference) {
            assert_eq!(
                result.verse_id,
                i64::try_from(*expected_index).expect("index fits i64")
            );
            assert!(
                (result.similarity - f64::from(*expected_similarity)).abs() < 1e-6,
                "similarity drift: {} vs {expected_similarity}",
                result.similarity
            );
        }
    }

    /// `k` larger than the corpus must clamp, not panic on the partial select.
    #[test]
    fn k_larger_than_corpus_returns_everything() {
        let index = HnswVectorIndex::from_vecs(
            vec![make_unit_vec(4, 0), make_unit_vec(4, 1)],
            vec![1, 2],
            4,
        )
        .unwrap();
        let results = index.search(&make_unit_vec(4, 0), 50).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].verse_id, 1);
    }

    #[test]
    fn test_dimension_mismatch() {
        let index = HnswVectorIndex::from_vecs(vec![vec![1.0, 0.0]], vec![1], 2).unwrap();
        let err = index.search(&[1.0, 0.0, 0.0], 1);
        assert!(err.is_err());
    }

    #[test]
    fn quantized_file_matches_f32_ranking() {
        let dim = 8;
        let vectors = vec![
            vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            vec![0.6, 0.8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ];
        let ids = vec![10, 20, 30];
        let dir = tempfile::tempdir().expect("temp dir");
        let embeddings_path = dir.path().join("q8.bin");
        let ids_path = dir.path().join("q8-ids.bin");
        let ids_bytes = write_ids(&ids_path, &ids);
        write_q8(&embeddings_path, &vectors, &ids_bytes, dim);

        let quantized = HnswVectorIndex::load(&embeddings_path, &ids_path, dim).expect("load q8");
        let f32_index = HnswVectorIndex::from_vecs(vectors, ids, dim).expect("build f32 index");
        let query = [0.6, 0.8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let q8_ids: Vec<i64> = quantized
            .search(&query, 3)
            .expect("q8 search")
            .into_iter()
            .map(|result| result.verse_id)
            .collect();
        let f32_ids: Vec<i64> = f32_index
            .search(&query, 3)
            .expect("f32 search")
            .into_iter()
            .map(|result| result.verse_id)
            .collect();

        assert_eq!(q8_ids, f32_ids);
    }

    #[test]
    fn quantized_file_rejects_mismatched_ids_digest() {
        let dim = 4;
        let vectors = vec![make_unit_vec(dim, 0)];
        let dir = tempfile::tempdir().expect("temp dir");
        let embeddings_path = dir.path().join("q8.bin");
        let ids_path = dir.path().join("q8-ids.bin");
        let original_ids = 10i64.to_le_bytes();
        write_q8(&embeddings_path, &vectors, &original_ids, dim);
        std::fs::write(&ids_path, 99i64.to_le_bytes()).expect("write changed ids");

        let error =
            HnswVectorIndex::load(&embeddings_path, &ids_path, dim).expect_err("digest mismatch");

        assert!(error.to_string().contains("IDs digest"));
    }

    #[test]
    fn quantized_file_rejects_unsupported_version_without_legacy_fallback() {
        let dim = 4;
        let vectors = vec![make_unit_vec(dim, 0)];
        let dir = tempfile::tempdir().expect("temp dir");
        let embeddings_path = dir.path().join("q8.bin");
        let ids_path = dir.path().join("q8-ids.bin");
        let ids_bytes = write_ids(&ids_path, &[10]);
        write_q8(&embeddings_path, &vectors, &ids_bytes, dim);
        let mut bytes = std::fs::read(&embeddings_path).expect("read q8");
        bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
        std::fs::write(&embeddings_path, bytes).expect("write unsupported version");

        let error = HnswVectorIndex::load(&embeddings_path, &ids_path, dim)
            .expect_err("unsupported q8 version");

        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn legacy_f32_file_still_loads() {
        let dim = 4;
        let dir = tempfile::tempdir().expect("temp dir");
        let embeddings_path = dir.path().join("legacy.bin");
        let ids_path = dir.path().join("legacy-ids.bin");
        let embeddings: Vec<f32> = make_unit_vec(dim, 2);
        std::fs::write(&embeddings_path, bytemuck::cast_slice(&embeddings))
            .expect("write embeddings");
        write_ids(&ids_path, &[42]);

        let index = HnswVectorIndex::load(&embeddings_path, &ids_path, dim).expect("load legacy");

        assert_eq!(index.len(), 1);
    }
}
