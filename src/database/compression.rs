//! Compression and content encoding helpers for database storage.
//!
//! Handles zstd compression (when the `compression` feature is enabled),
//! base64 encoding/decoding, and content decompression for stored documents.

use super::B64;
use crate::error::FactbaseError;
use base64::Engine;

/// Compression prefix for zstd-compressed content
#[cfg(feature = "compression")]
pub(crate) const ZSTD_PREFIX: &[u8] = b"ZSTD:";

/// Compress content using zstd
#[cfg(feature = "compression")]
pub(crate) fn compress_content(content: &str) -> Vec<u8> {
    let mut result = ZSTD_PREFIX.to_vec();
    let compressed =
        zstd::encode_all(content.as_bytes(), 3).unwrap_or_else(|_| content.as_bytes().to_vec());
    result.extend(compressed);
    result
}

/// No-op compression when feature disabled
#[cfg(not(feature = "compression"))]
pub(crate) fn compress_content(content: &str) -> Vec<u8> {
    content.as_bytes().to_vec()
}

/// Decompress content, auto-detecting if compressed
#[cfg(feature = "compression")]
pub(crate) fn decompress_content(data: &[u8]) -> Result<String, FactbaseError> {
    if data.starts_with(ZSTD_PREFIX) {
        let compressed = &data[ZSTD_PREFIX.len()..];
        let decompressed = zstd::decode_all(compressed)
            .map_err(|e| FactbaseError::internal(format!("decompression failed: {e}")))?;
        String::from_utf8(decompressed)
            .map_err(|e| FactbaseError::internal(format!("invalid UTF-8 after decompression: {e}")))
    } else {
        // Uncompressed - treat as UTF-8 string
        String::from_utf8(data.to_vec())
            .map_err(|e| FactbaseError::internal(format!("invalid UTF-8: {e}")))
    }
}

/// Decode content, auto-detecting if it's compressed (base64-encoded zstd)
pub(crate) fn decode_content(stored: &str) -> Result<String, FactbaseError> {
    // Try to decode as base64 - if it succeeds and starts with ZSTD prefix, decompress
    if let Ok(decoded) = B64.decode(stored) {
        #[cfg(feature = "compression")]
        {
            if decoded.starts_with(ZSTD_PREFIX) {
                return decompress_content(&decoded);
            }
        }
        // If we got valid base64 but it's not compressed, try to interpret as UTF-8
        if let Ok(s) = String::from_utf8(decoded) {
            return Ok(s);
        }
    }
    // Not compressed, return as-is
    Ok(stored.to_string())
}

/// Decode content, returning the original string if decoding fails.
pub(crate) fn decode_content_lossy(stored: String) -> String {
    decode_content(&stored).unwrap_or(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "compression")]
    fn test_compress_decompress_roundtrip() {
        let original =
            "This is some test content that should be compressed and decompressed correctly.";
        let compressed = compress_content(original);

        // Compressed data should start with ZSTD prefix
        assert!(
            compressed.starts_with(ZSTD_PREFIX),
            "Compressed data should have ZSTD prefix"
        );

        // Decompress should return original
        let decompressed = decompress_content(&compressed).expect("decompression should succeed");
        assert_eq!(decompressed, original, "Roundtrip should preserve content");
    }

    #[test]
    #[cfg(feature = "compression")]
    fn test_decompress_uncompressed_data() {
        let plain = "Plain text without compression";
        let result = decompress_content(plain.as_bytes()).expect("should handle uncompressed data");
        assert_eq!(result, plain, "Uncompressed data should pass through");
    }

    #[test]
    #[cfg(feature = "compression")]
    fn test_decode_content_compressed() {
        let original = "Test content for compression";
        let compressed = compress_content(original);
        let encoded = B64.encode(&compressed);

        let decoded = decode_content(&encoded).expect("decode should succeed");
        assert_eq!(
            decoded, original,
            "Encoded compressed content should decode correctly"
        );
    }

    #[test]
    fn test_decode_content_uncompressed() {
        let plain = "Plain text content";
        let decoded = decode_content(plain).expect("decode should succeed");
        assert_eq!(decoded, plain, "Plain text should pass through unchanged");
    }

    #[test]
    fn test_decode_content_lossy_plain() {
        let plain = "Plain text content".to_string();
        assert_eq!(decode_content_lossy(plain), "Plain text content");
    }
}
