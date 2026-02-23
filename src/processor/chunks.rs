//! Document chunking for large documents.
//!
//! This module handles splitting large documents into chunks for embedding
//! generation when they exceed the model's context window.

/// A chunk of a document for embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentChunk {
    /// Zero-based chunk index
    pub index: usize,
    /// Start byte offset in the original content
    pub start: usize,
    /// End byte offset in the original content
    pub end: usize,
    /// The chunk text content
    pub content: String,
}

/// Split document into overlapping chunks for embedding.
/// Returns single chunk if content is smaller than chunk_size.
pub fn chunk_document(content: &str, chunk_size: usize, overlap: usize) -> Vec<DocumentChunk> {
    if content.is_empty() {
        return vec![DocumentChunk {
            index: 0,
            start: 0,
            end: 0,
            content: String::new(),
        }];
    }

    if content.len() <= chunk_size {
        return vec![DocumentChunk {
            index: 0,
            start: 0,
            end: content.len(),
            content: content.to_string(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while start < content.len() {
        let mut end = (start + chunk_size).min(content.len());

        // Find word boundary (don't split mid-word)
        if end < content.len() {
            if let Some(pos) = content[start..end].rfind(char::is_whitespace) {
                end = start + pos + 1; // Include the whitespace
            }
        }

        chunks.push(DocumentChunk {
            index,
            start,
            end,
            content: content[start..end].to_string(),
        });

        if end >= content.len() {
            break;
        }

        // Next chunk starts at (end - overlap), but find word boundary
        let next_start = if end > overlap {
            let candidate = end - overlap;
            // Find next word boundary after candidate
            if let Some(pos) = content[candidate..end].find(char::is_whitespace) {
                candidate + pos + 1
            } else {
                candidate
            }
        } else {
            end
        };

        start = next_start;
        index += 1;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_document_small() {
        let content = "Small document";
        let chunks = chunk_document(content, 100, 20);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].end, content.len());
        assert_eq!(chunks[0].content, content);
    }

    #[test]
    fn test_chunk_document_empty() {
        let chunks = chunk_document("", 100, 20);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "");
    }

    #[test]
    fn test_chunk_document_multiple() {
        let content = "word1 word2 word3 word4 word5 word6 word7 word8";
        let chunks = chunk_document(content, 20, 5);
        assert!(chunks.len() >= 2);
        // First chunk
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].start, 0);
        // All content covered
        let last = chunks.last().expect("should have chunks");
        assert_eq!(last.end, content.len());
    }

    #[test]
    fn test_chunk_document_word_boundary() {
        let content = "hello world this is a test";
        let chunks = chunk_document(content, 12, 3);
        // Should not split mid-word - each chunk should end at whitespace or end of content
        for chunk in &chunks {
            let trimmed = chunk.content.trim_end();
            // Content should not end with a partial word (unless it's the last chunk)
            if chunk.end < content.len() {
                assert!(
                    chunk.content.ends_with(char::is_whitespace)
                        || chunk.content.ends_with(|c: char| !c.is_alphabetic()),
                    "Chunk should end at word boundary: {:?}",
                    trimmed
                );
            }
        }
    }

    #[test]
    fn test_chunk_document_overlap() {
        let content = "aaa bbb ccc ddd eee fff ggg hhh iii jjj";
        let chunks = chunk_document(content, 15, 5);
        // With overlap, chunks should share some content
        if chunks.len() >= 2 {
            let end_of_first = chunks[0].end;
            let start_of_second = chunks[1].start;
            assert!(
                start_of_second < end_of_first,
                "Chunks should overlap: first ends at {}, second starts at {}",
                end_of_first,
                start_of_second
            );
        }
    }
}
