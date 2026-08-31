use std::fmt;
use std::str::FromStr;

/// Tags for memory entries, providing structured metadata.
///
/// Wire format: `"prefix:value"` (e.g., `"node_id:abc"`, `"turn:5"`).
/// Serializes to/from plain strings for backward-compatible JSON.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum MemoryTag {
    NodeId(String),
    Turn(u64),
    SignalId(String),
    Scope(String),
    Entity(String),
    Context(String),
    Custom(String),
}

impl fmt::Display for MemoryTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeId(v) => write!(f, "node_id:{v}"),
            Self::Turn(v) => write!(f, "turn:{v}"),
            Self::SignalId(v) => write!(f, "signal_id:{v}"),
            Self::Scope(v) => write!(f, "scope:{v}"),
            Self::Entity(v) => write!(f, "entity:{v}"),
            Self::Context(v) => write!(f, "context:{v}"),
            Self::Custom(v) => write!(f, "custom:{v}"),
        }
    }
}

impl From<MemoryTag> for String {
    fn from(tag: MemoryTag) -> String {
        tag.to_string()
    }
}

/// Parse error for invalid tag strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTagError(String);

impl fmt::Display for ParseTagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid tag format: {}", self.0)
    }
}

impl std::error::Error for ParseTagError {}

impl FromStr for MemoryTag {
    type Err = ParseTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((prefix, value)) = s.split_once(':') else {
            return Err(ParseTagError(s.to_string()));
        };
        match prefix {
            "node_id" => Ok(Self::NodeId(value.to_string())),
            "turn" => value
                .parse::<u64>()
                .map(Self::Turn)
                .map_err(|_| ParseTagError(s.to_string())),
            "signal_id" => Ok(Self::SignalId(value.to_string())),
            "scope" => Ok(Self::Scope(value.to_string())),
            "entity" => Ok(Self::Entity(value.to_string())),
            "context" => Ok(Self::Context(value.to_string())),
            "custom" => Ok(Self::Custom(value.to_string())),
            _ => Err(ParseTagError(s.to_string())),
        }
    }
}

impl TryFrom<String> for MemoryTag {
    type Error = ParseTagError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Shorthand macro for constructing [`MemoryTag`] values.
///
/// ```
/// use mcl_memory::tag;
/// use mcl_memory::MemoryTag;
///
/// let t = tag!(node_id = "abc");
/// assert_eq!(t, MemoryTag::NodeId("abc".into()));
///
/// let t = tag!(turn = 5);
/// assert_eq!(t, MemoryTag::Turn(5));
/// ```
#[macro_export]
macro_rules! tag {
    (node_id = $v:expr) => {
        $crate::tags::MemoryTag::NodeId($v.into())
    };
    (turn = $v:expr) => {
        $crate::tags::MemoryTag::Turn($v)
    };
    (signal_id = $v:expr) => {
        $crate::tags::MemoryTag::SignalId($v.into())
    };
    (scope = $v:expr) => {
        $crate::tags::MemoryTag::Scope($v.into())
    };
    (entity = $v:expr) => {
        $crate::tags::MemoryTag::Entity($v.into())
    };
    (context = $v:expr) => {
        $crate::tags::MemoryTag::Context($v.into())
    };
    (custom = $v:expr) => {
        $crate::tags::MemoryTag::Custom($v.into())
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_roundtrip_all_variants() {
        let tags = vec![
            MemoryTag::NodeId("abc".into()),
            MemoryTag::Turn(42),
            MemoryTag::SignalId("blake3hash".into()),
            MemoryTag::Scope("combat".into()),
            MemoryTag::Entity("skeleton_guard".into()),
            MemoryTag::Context("crypt_antechamber".into()),
            MemoryTag::Custom("anything".into()),
        ];
        for tag in &tags {
            let s = tag.to_string();
            let parsed: MemoryTag = s.parse().expect("should parse");
            assert_eq!(&parsed, tag, "round-trip failed for {s}");
        }
    }

    #[test]
    fn from_str_errors_on_missing_colon() {
        assert!("nocolon".parse::<MemoryTag>().is_err());
    }

    #[test]
    fn from_str_errors_on_unknown_prefix() {
        assert!("unknown:value".parse::<MemoryTag>().is_err());
    }

    #[test]
    fn from_str_errors_on_non_numeric_turn() {
        assert!("turn:not_a_number".parse::<MemoryTag>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let tag = MemoryTag::Scope("exploration".into());
        let json = serde_json::to_string(&tag).expect("serialize");
        assert_eq!(json, "\"scope:exploration\"");
        let back: MemoryTag = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, tag);
    }

    #[test]
    fn serde_vec_roundtrip() {
        let tags = vec![
            MemoryTag::Scope("combat".into()),
            MemoryTag::Turn(5),
            MemoryTag::Entity("goblin".into()),
        ];
        let json = serde_json::to_string(&tags).expect("serialize");
        let back: Vec<MemoryTag> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, tags);
    }

    #[test]
    fn tag_macro_constructs_variants() {
        assert_eq!(tag!(node_id = "abc"), MemoryTag::NodeId("abc".into()));
        assert_eq!(tag!(turn = 5), MemoryTag::Turn(5));
        assert_eq!(tag!(signal_id = "h"), MemoryTag::SignalId("h".into()));
        assert_eq!(tag!(scope = "combat"), MemoryTag::Scope("combat".into()));
        assert_eq!(tag!(entity = "x"), MemoryTag::Entity("x".into()));
        assert_eq!(tag!(context = "room"), MemoryTag::Context("room".into()));
        assert_eq!(tag!(custom = "foo"), MemoryTag::Custom("foo".into()));
    }
}
