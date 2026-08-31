use mcl_spec::value::MclValue;

use crate::tags::MemoryTag;

/// Result of chunking a payload for memory storage
pub struct ChunkResult {
    pub content: MclValue,
    pub text: String,
    pub tags: Vec<MemoryTag>,
}

/// Breaks packet payloads into memory entries
pub trait Chunker: Send + Sync {
    fn chunk(&self, channel: &str, payload: &MclValue) -> Vec<ChunkResult>;
}

/// Default: store the entire payload as one entry
pub struct WholePacketChunker;

impl Chunker for WholePacketChunker {
    fn chunk(&self, channel: &str, payload: &MclValue) -> Vec<ChunkResult> {
        let text = extract_text(payload);
        vec![ChunkResult {
            content: payload.clone(),
            text,
            tags: vec![MemoryTag::Custom(channel.to_string())],
        }]
    }
}

/// Split text content by paragraph (double newline)
pub struct ParagraphChunker;

impl Chunker for ParagraphChunker {
    fn chunk(&self, channel: &str, payload: &MclValue) -> Vec<ChunkResult> {
        let full_text = extract_text(payload);
        let paragraphs: Vec<&str> = full_text
            .split("\n\n")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if paragraphs.is_empty() {
            return vec![ChunkResult {
                content: payload.clone(),
                text: full_text,
                tags: vec![MemoryTag::Custom(channel.to_string())],
            }];
        }

        paragraphs
            .into_iter()
            .map(|para| ChunkResult {
                content: MclValue::String(para.to_string()),
                text: para.to_string(),
                tags: vec![MemoryTag::Custom(channel.to_string())],
            })
            .collect()
    }
}

/// One entry per MCL signal in the payload
pub struct SignalChunker;

impl Chunker for SignalChunker {
    fn chunk(&self, channel: &str, payload: &MclValue) -> Vec<ChunkResult> {
        // Try to extract signals array from payload
        if let Some(map) = payload.as_map() {
            if let Some(signals) = map.get("signals").and_then(|v| v.as_list()) {
                return signals
                    .iter()
                    .map(|signal| ChunkResult {
                        content: signal.clone(),
                        text: signal.to_string(),
                        tags: vec![
                            MemoryTag::Custom(channel.to_string()),
                            MemoryTag::Custom("signal".to_string()),
                        ],
                    })
                    .collect();
            }
        }
        // Fallback: whole payload
        WholePacketChunker.chunk(channel, payload)
    }
}

/// Extract a text representation from an MclValue for embedding
fn extract_text(value: &MclValue) -> String {
    match value {
        MclValue::String(s) => s.clone(),
        MclValue::Map(m) => {
            // Try common text fields first
            for key in &["text", "content", "message", "narration"] {
                if let Some(MclValue::String(s)) = m.get(*key) {
                    return s.clone();
                }
            }
            // Fall back to stringifying the whole map
            value.to_string()
        }
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_map(json: &str) -> MclValue {
        serde_json::from_str(json).expect("valid json")
    }

    #[test]
    fn whole_packet_chunker_string_value() {
        let chunker = WholePacketChunker;
        let payload = MclValue::String("hello world".into());
        let results = chunker.chunk("test_channel", &payload);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "hello world");
        assert_eq!(
            results[0].tags,
            vec![MemoryTag::Custom("test_channel".into())]
        );
    }

    #[test]
    fn paragraph_chunker_splits_on_double_newline() {
        let chunker = ParagraphChunker;
        let payload = MclValue::String("para one\n\npara two\n\npara three".into());
        let results = chunker.chunk("narrative", &payload);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].text, "para one");
        assert_eq!(results[1].text, "para two");
        assert_eq!(results[2].text, "para three");
        for r in &results {
            assert_eq!(r.tags, vec![MemoryTag::Custom("narrative".into())]);
        }
    }

    #[test]
    fn paragraph_chunker_no_double_newline_returns_single() {
        let chunker = ParagraphChunker;
        let payload = MclValue::String("just one paragraph".into());
        let results = chunker.chunk("channel", &payload);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "just one paragraph");
    }

    #[test]
    fn signal_chunker_extracts_signals() {
        let chunker = SignalChunker;
        let payload = json_map(r#"{"signals": ["signal_a", "signal_b"]}"#);
        let results = chunker.chunk("combat", &payload);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text, "signal_a");
        assert_eq!(
            results[1].tags,
            vec![
                MemoryTag::Custom("combat".into()),
                MemoryTag::Custom("signal".into())
            ]
        );
    }

    #[test]
    fn extract_text_prefers_text_field() {
        let m = json_map(r#"{"text": "extracted", "other": 42}"#);
        assert_eq!(extract_text(&m), "extracted");
    }
}
