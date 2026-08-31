use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MclValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<MclValue>),
    Map(IndexMap<String, MclValue>),
}

impl MclValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            MclValue::Null => false,
            MclValue::Bool(b) => *b,
            MclValue::Int(n) => *n != 0,
            MclValue::Float(n) => *n != 0.0,
            MclValue::String(s) => !s.is_empty(),
            MclValue::List(l) => !l.is_empty(),
            MclValue::Map(m) => !m.is_empty(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            MclValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            MclValue::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            MclValue::Float(n) => Some(*n),
            MclValue::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            MclValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[MclValue]> {
        match self {
            MclValue::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&IndexMap<String, MclValue>> {
        match self {
            MclValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            MclValue::Null => "null",
            MclValue::Bool(_) => "bool",
            MclValue::Int(_) => "int",
            MclValue::Float(_) => "float",
            MclValue::String(_) => "string",
            MclValue::List(_) => "list",
            MclValue::Map(_) => "map",
        }
    }
}

impl std::fmt::Display for MclValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MclValue::Null => write!(f, "null"),
            MclValue::Bool(b) => write!(f, "{b}"),
            MclValue::Int(n) => write!(f, "{n}"),
            MclValue::Float(n) => write!(f, "{n}"),
            MclValue::String(s) => write!(f, "{s}"),
            MclValue::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            MclValue::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl From<&str> for MclValue {
    fn from(s: &str) -> Self {
        MclValue::String(s.to_string())
    }
}

impl From<String> for MclValue {
    fn from(s: String) -> Self {
        MclValue::String(s)
    }
}

impl From<i64> for MclValue {
    fn from(n: i64) -> Self {
        MclValue::Int(n)
    }
}

impl From<f64> for MclValue {
    fn from(n: f64) -> Self {
        MclValue::Float(n)
    }
}

impl From<bool> for MclValue {
    fn from(b: bool) -> Self {
        MclValue::Bool(b)
    }
}

/// A value with associated flow labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabeledValue {
    pub value: MclValue,
    #[serde(default)]
    pub labels: crate::label::LabelSet,
}

impl std::fmt::Display for LabeledValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)?;
        for label in &self.labels.0 {
            write!(f, " @{name}", name = label.name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_null() {
        assert_eq!(MclValue::Null.to_string(), "null");
    }

    #[test]
    fn display_primitives() {
        assert_eq!(MclValue::Bool(true).to_string(), "true");
        assert_eq!(MclValue::Bool(false).to_string(), "false");
        assert_eq!(MclValue::Int(42).to_string(), "42");
        assert_eq!(MclValue::Int(-7).to_string(), "-7");
        assert_eq!(MclValue::Float(3.14).to_string(), "3.14");
        assert_eq!(MclValue::String("hello".into()).to_string(), "hello");
    }

    #[test]
    fn display_list() {
        let list = MclValue::List(vec![MclValue::Int(1), MclValue::Int(2), MclValue::Int(3)]);
        assert_eq!(list.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn display_map() {
        let mut map = indexmap::IndexMap::new();
        map.insert("x".into(), MclValue::Int(1));
        map.insert("y".into(), MclValue::Int(2));
        assert_eq!(MclValue::Map(map).to_string(), "{x: 1, y: 2}");
    }

    #[test]
    fn serde_round_trip() {
        let values = vec![
            MclValue::Null,
            MclValue::Bool(true),
            MclValue::Int(42),
            MclValue::Float(3.14),
            MclValue::String("hello".into()),
            MclValue::List(vec![MclValue::Int(1), MclValue::Null]),
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let back: MclValue = serde_json::from_str(&json).unwrap();
            assert_eq!(v, &back, "round-trip failed for {v}");
        }
    }

    #[test]
    fn is_truthy() {
        assert!(!MclValue::Null.is_truthy());
        assert!(!MclValue::Bool(false).is_truthy());
        assert!(MclValue::Bool(true).is_truthy());
        assert!(!MclValue::Int(0).is_truthy());
        assert!(MclValue::Int(1).is_truthy());
        assert!(!MclValue::Float(0.0).is_truthy());
        assert!(MclValue::Float(1.0).is_truthy());
        assert!(!MclValue::String("".into()).is_truthy());
        assert!(MclValue::String("hi".into()).is_truthy());
        assert!(!MclValue::List(vec![]).is_truthy());
        assert!(MclValue::List(vec![MclValue::Null]).is_truthy());
        assert!(!MclValue::Map(indexmap::IndexMap::new()).is_truthy());
    }

    #[test]
    fn type_name_matches() {
        assert_eq!(MclValue::Null.type_name(), "null");
        assert_eq!(MclValue::Bool(false).type_name(), "bool");
        assert_eq!(MclValue::Int(0).type_name(), "int");
        assert_eq!(MclValue::Float(0.0).type_name(), "float");
        assert_eq!(MclValue::String("".into()).type_name(), "string");
        assert_eq!(MclValue::List(vec![]).type_name(), "list");
        assert_eq!(MclValue::Map(indexmap::IndexMap::new()).type_name(), "map");
    }

    #[test]
    fn as_accessors() {
        assert_eq!(MclValue::Int(5).as_int(), Some(5));
        assert_eq!(MclValue::Bool(true).as_int(), None);
        assert_eq!(MclValue::Float(2.5).as_float(), Some(2.5));
        assert_eq!(MclValue::Int(3).as_float(), Some(3.0));
        assert_eq!(MclValue::String("x".into()).as_str(), Some("x"));
        assert_eq!(MclValue::Bool(true).as_bool(), Some(true));
        assert_eq!(
            MclValue::List(vec![MclValue::Int(1)])
                .as_list()
                .map(|l| l.len()),
            Some(1)
        );
        assert!(MclValue::Int(0).as_list().is_none());
    }

    #[test]
    fn from_impls() {
        let v: MclValue = "hello".into();
        assert_eq!(v, MclValue::String("hello".into()));
        let v: MclValue = String::from("world").into();
        assert_eq!(v, MclValue::String("world".into()));
        let v: MclValue = 42i64.into();
        assert_eq!(v, MclValue::Int(42));
        let v: MclValue = 3.14f64.into();
        assert!(matches!(v, MclValue::Float(_)));
        let v: MclValue = true.into();
        assert_eq!(v, MclValue::Bool(true));
    }
}
