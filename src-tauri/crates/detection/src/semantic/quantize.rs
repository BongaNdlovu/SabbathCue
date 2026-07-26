//! Per-vector int8 quantization and the self-identifying `SCQ8` file header.

use sha2::{Digest, Sha256};

/// Identifies a quantized embeddings file.
pub const Q8_MAGIC: [u8; 4] = *b"SCQ8";
/// Current on-disk format version.
pub const Q8_VERSION: u32 = 1;
/// Fixed header size for version 1.
pub const Q8_HEADER_LEN: usize = 56;

const Q8_MAX: f32 = 127.0;
const IDS_DIGEST_LEN: usize = 32;

/// Metadata stored before the scales and quantized vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Q8Header {
    /// Number of components in each stored vector.
    pub dim: usize,
    /// Number of stored vectors.
    pub num_vectors: usize,
    /// SHA-256 digest binding the embeddings to their ordered IDs file.
    pub ids_sha256: [u8; IDS_DIGEST_LEN],
}

/// Errors produced while quantizing vectors or decoding the q8 format.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Q8Error {
    /// A vector component cannot be represented safely.
    #[error("vector component {index} is not finite")]
    NonFiniteComponent {
        /// Component offset within the vector.
        index: usize,
    },
    /// Header dimensions must fit the portable u32 representation.
    #[error("embedding dimension {0} does not fit the q8 format")]
    DimensionOutOfRange(usize),
    /// Header vector counts must fit the portable u32 representation.
    #[error("vector count {0} does not fit the q8 format")]
    VectorCountOutOfRange(usize),
    /// A file beginning with `SCQ8` does not contain the complete header.
    #[error("quantized embeddings header is truncated")]
    TruncatedHeader,
    /// A future format must never be interpreted as legacy f32 data.
    #[error("unsupported quantized embeddings version {0}")]
    UnsupportedVersion(u32),
    /// The encoded header length is inconsistent with this version.
    #[error("quantized embeddings header length {0} is invalid")]
    InvalidHeaderLength(u32),
    /// Reserved bits must remain zero until a future format defines them.
    #[error("quantized embeddings reserved field must be zero")]
    ReservedFieldSet,
    /// Zero-dimensional vectors are not meaningful.
    #[error("quantized embeddings dimension must be greater than zero")]
    ZeroDimension,
    /// A q8 asset must contain at least one vector.
    #[error("quantized embeddings vector count must be greater than zero")]
    ZeroVectorCount,
}

/// Compute the digest used to bind an embeddings file to its ordered IDs.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; IDS_DIGEST_LEN] {
    Sha256::digest(bytes).into()
}

/// Quantize one vector with a scale derived from its largest magnitude.
///
/// # Errors
///
/// Returns [`Q8Error::NonFiniteComponent`] if the vector contains NaN or
/// infinity.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the rounded value is explicitly clamped to the i8-safe symmetric range"
)]
pub fn quantize_vector(vector: &[f32]) -> Result<(Vec<i8>, f32), Q8Error> {
    let mut peak = 0.0f32;
    for (index, component) in vector.iter().copied().enumerate() {
        if !component.is_finite() {
            return Err(Q8Error::NonFiniteComponent { index });
        }
        peak = peak.max(component.abs());
    }
    if peak == 0.0 {
        return Ok((vec![0; vector.len()], 0.0));
    }

    let scale = peak / Q8_MAX;
    let data = vector
        .iter()
        .map(|component| {
            let scaled = (component / scale).round();
            scaled.clamp(-Q8_MAX, Q8_MAX) as i8
        })
        .collect();
    Ok((data, scale))
}

/// Reconstruct approximate f32 components for validation and diagnostics.
#[must_use]
pub fn dequantize_vector(data: &[i8], scale: f32) -> Vec<f32> {
    data.iter()
        .map(|component| f32::from(*component) * scale)
        .collect()
}

/// Encode a portable little-endian q8 header.
///
/// # Errors
///
/// Returns an error when `dim` or `num_vectors` is zero or cannot fit in the
/// portable u32 header fields.
pub fn encode_header(
    dim: usize,
    num_vectors: usize,
    ids_sha256: [u8; IDS_DIGEST_LEN],
) -> Result<[u8; Q8_HEADER_LEN], Q8Error> {
    if dim == 0 {
        return Err(Q8Error::ZeroDimension);
    }
    if num_vectors == 0 {
        return Err(Q8Error::ZeroVectorCount);
    }
    let dim = u32::try_from(dim).map_err(|_| Q8Error::DimensionOutOfRange(dim))?;
    let num_vectors =
        u32::try_from(num_vectors).map_err(|_| Q8Error::VectorCountOutOfRange(num_vectors))?;
    let header_len =
        u32::try_from(Q8_HEADER_LEN).map_err(|_| Q8Error::DimensionOutOfRange(Q8_HEADER_LEN))?;

    let mut bytes = [0u8; Q8_HEADER_LEN];
    bytes[0..4].copy_from_slice(&Q8_MAGIC);
    bytes[4..8].copy_from_slice(&Q8_VERSION.to_le_bytes());
    bytes[8..12].copy_from_slice(&header_len.to_le_bytes());
    bytes[12..16].copy_from_slice(&dim.to_le_bytes());
    bytes[16..20].copy_from_slice(&num_vectors.to_le_bytes());
    // bytes[20..24] is reserved and remains zero.
    bytes[24..56].copy_from_slice(&ids_sha256);
    Ok(bytes)
}

/// Probe an embeddings prefix for the q8 header.
///
/// `Ok(None)` is reserved for files without `SCQ8` magic, allowing legacy f32
/// loading. Once the magic is present, malformed or unsupported data is an
/// error and must never fall through to the legacy loader.
///
/// # Errors
///
/// Returns a [`Q8Error`] for any invalid header that begins with `SCQ8`.
pub fn decode_header(bytes: &[u8]) -> Result<Option<Q8Header>, Q8Error> {
    if bytes.len() < Q8_MAGIC.len() || bytes[0..4] != Q8_MAGIC {
        return Ok(None);
    }
    if bytes.len() < Q8_HEADER_LEN {
        return Err(Q8Error::TruncatedHeader);
    }

    let version = u32::from_le_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| Q8Error::TruncatedHeader)?,
    );
    if version != Q8_VERSION {
        return Err(Q8Error::UnsupportedVersion(version));
    }
    let header_len = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| Q8Error::TruncatedHeader)?,
    );
    if usize::try_from(header_len).ok() != Some(Q8_HEADER_LEN) {
        return Err(Q8Error::InvalidHeaderLength(header_len));
    }
    let dim = u32::from_le_bytes(
        bytes[12..16]
            .try_into()
            .map_err(|_| Q8Error::TruncatedHeader)?,
    );
    if dim == 0 {
        return Err(Q8Error::ZeroDimension);
    }
    let num_vectors = u32::from_le_bytes(
        bytes[16..20]
            .try_into()
            .map_err(|_| Q8Error::TruncatedHeader)?,
    );
    if num_vectors == 0 {
        return Err(Q8Error::ZeroVectorCount);
    }
    if bytes[20..24] != [0; 4] {
        return Err(Q8Error::ReservedFieldSet);
    }

    let mut ids_sha256 = [0u8; IDS_DIGEST_LEN];
    ids_sha256.copy_from_slice(&bytes[24..56]);
    Ok(Some(Q8Header {
        dim: dim as usize,
        num_vectors: num_vectors as usize,
        ids_sha256,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        decode_header, dequantize_vector, encode_header, quantize_vector, sha256, Q8Error,
        Q8_HEADER_LEN, Q8_MAGIC,
    };

    fn normalised(seed: u64, dim: usize) -> Vec<f32> {
        let mut state = seed | 1;
        let mut vector: Vec<f32> = (0..dim)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let bits = u16::try_from(state >> 48).expect("shifted value fits u16");
                f32::from(bits) / 65_535.0 - 0.5
            })
            .collect();
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        for value in &mut vector {
            *value /= norm;
        }
        vector
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(left, right)| left * right).sum();
        let norm_a = a.iter().map(|value| value * value).sum::<f32>().sqrt();
        let norm_b = b.iter().map(|value| value * value).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    }

    #[test]
    fn quantize_vector_preserves_cosine_within_tolerance() {
        for seed in 1..20 {
            let original = normalised(seed, 384);
            let (data, scale) = quantize_vector(&original).expect("finite vector");
            let restored = dequantize_vector(&data, scale);
            assert!(
                cosine(&original, &restored) > 0.9999,
                "seed {seed} exceeded the quantization error budget"
            );
        }
    }

    #[test]
    fn quantize_vector_uses_full_symmetric_range() {
        let (data, _) = quantize_vector(&normalised(7, 384)).expect("finite vector");
        let peak = data
            .iter()
            .map(|component| component.unsigned_abs())
            .max()
            .expect("non-empty vector");
        assert_eq!(peak, 127);
    }

    #[test]
    fn quantize_vector_handles_zero_vector_without_nan() {
        let (data, scale) = quantize_vector(&[0.0; 384]).expect("zero is finite");
        assert!(scale.abs() <= f32::EPSILON);
        assert!(data.iter().all(|component| *component == 0));
    }

    #[test]
    fn quantize_vector_rejects_non_finite_component() {
        assert_eq!(
            quantize_vector(&[0.0, f32::NAN]),
            Err(Q8Error::NonFiniteComponent { index: 1 })
        );
    }

    #[test]
    fn header_round_trips_and_binds_ids() {
        let ids_sha256 = sha256(b"ordered ids");
        let bytes = encode_header(384, 62_197, ids_sha256).expect("valid header");
        let header = decode_header(&bytes)
            .expect("valid format")
            .expect("q8 header");

        assert_eq!(bytes.len(), Q8_HEADER_LEN);
        assert_eq!(&bytes[0..4], &Q8_MAGIC);
        assert_eq!(
            header,
            super::Q8Header {
                dim: 384,
                num_vectors: 62_197,
                ids_sha256,
            }
        );
    }

    #[test]
    fn decoder_reports_legacy_when_magic_is_absent() {
        let bytes = 0.05f32.to_le_bytes();
        assert_eq!(decode_header(&bytes), Ok(None));
    }

    #[test]
    fn decoder_rejects_unsupported_q8_version_instead_of_falling_back() {
        let mut bytes = encode_header(384, 10, sha256(b"ids")).expect("valid header");
        bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(decode_header(&bytes), Err(Q8Error::UnsupportedVersion(99)));
    }

    #[test]
    fn decoder_rejects_truncated_q8_header() {
        let bytes = encode_header(384, 10, sha256(b"ids")).expect("valid header");
        assert_eq!(
            decode_header(&bytes[..Q8_HEADER_LEN - 1]),
            Err(Q8Error::TruncatedHeader)
        );
    }

    #[test]
    fn decoder_rejects_nonzero_reserved_field() {
        let mut bytes = encode_header(384, 10, sha256(b"ids")).expect("valid header");
        bytes[20] = 1;
        assert_eq!(decode_header(&bytes), Err(Q8Error::ReservedFieldSet));
    }
}
