use mcl_memory::create_mock_embedder;
use mcl_memory::tags::MemoryTag;
use mcl_memory::MemoryStore;
use mcl_spec::value::MclValue;
use proptest::prelude::*;

const MOCK_DIMS: usize = 384;

proptest! {
    #[test]
    fn prop_recent_returns_last_n_in_insertion_order(
        contents in prop::collection::vec("[a-z]{1,20}", 1..50),
        n in 0..10usize
    ) {
        let mut store = MemoryStore::new(100, create_mock_embedder(MOCK_DIMS), MOCK_DIMS)
            .expect("store creation should succeed");

        for content in &contents {
            store.insert(
                MclValue::String(content.clone()),
                content,
                vec![],
            );
        }

        let recent = store.recent(n);
        let expected_n = n.min(contents.len());

        assert_eq!(recent.len(), expected_n);

        for (i, entry) in recent.iter().enumerate() {
            let expected_idx = contents.len() - 1 - i;
            assert_eq!(entry.text, contents[expected_idx]);
        }
    }

    #[test]
    fn prop_memory_tag_display_fromstr_roundtrip(
        variant in 0..7usize,
        value in "[a-z0-9_]{1,20}"
    ) {
        let tag = match variant {
            0 => MemoryTag::NodeId(value.clone()),
            1 => MemoryTag::Turn(value.parse::<u64>().unwrap_or(42)),
            2 => MemoryTag::SignalId(value.clone()),
            3 => MemoryTag::Scope(value.clone()),
            4 => MemoryTag::Entity(value.clone()),
            5 => MemoryTag::Context(value.clone()),
            6 => MemoryTag::Custom(value.clone()),
            _ => unreachable!(),
        };

        let s = tag.to_string();
        let parsed: Result<MemoryTag, _> = s.parse();

        assert!(parsed.is_ok(), "Failed to parse: {}", s);

        let parsed = parsed.unwrap();
        assert_eq!(parsed, tag, "Round-trip failed: {} -> {} -> {:?}", tag, s, parsed);
    }

    #[test]
    fn prop_recent_with_capacity_overflow(
        contents in prop::collection::vec("[a-z]{1,20}", 3..20),
        capacity in 2..5usize
    ) {
        let mut store = MemoryStore::new(capacity, create_mock_embedder(MOCK_DIMS), MOCK_DIMS)
            .expect("store creation should succeed");

        for content in &contents {
            store.insert(
                MclValue::String(content.clone()),
                content,
                vec![],
            );
        }

        assert!(store.len() <= capacity);

        let recent = store.recent(store.len());
        assert_eq!(recent.len(), store.len());

        let recent_texts: Vec<&str> = recent.iter().map(|e| e.text.as_str()).collect();
        let stored_texts: Vec<&str> = store.recent(store.len()).iter().map(|e| e.text.as_str()).collect();

        assert_eq!(recent_texts, stored_texts);
    }

    #[test]
    fn prop_recent_n_greater_than_len(
        contents in prop::collection::vec("[a-z]{1,20}", 1..10)
    ) {
        let mut store = MemoryStore::new(100, create_mock_embedder(MOCK_DIMS), MOCK_DIMS)
            .expect("store creation should succeed");

        for content in &contents {
            store.insert(
                MclValue::String(content.clone()),
                content,
                vec![],
            );
        }

        let n = contents.len() + 10;
        let recent = store.recent(n);

        assert_eq!(recent.len(), contents.len());

        for (i, entry) in recent.iter().enumerate() {
            let expected_idx = contents.len() - 1 - i;
            assert_eq!(entry.text, contents[expected_idx]);
        }
    }
}
