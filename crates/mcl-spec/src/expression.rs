use serde::{Deserialize, Serialize};

use crate::value::MclValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldPath(pub Vec<String>);

impl FieldPath {
    pub fn new(segments: Vec<String>) -> Self {
        Self(segments)
    }

    pub fn single(name: impl Into<String>) -> Self {
        Self(vec![name.into()])
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }
}

impl std::fmt::Display for FieldPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for seg in &self.0 {
            if !first {
                write!(f, ".")?;
            }
            write!(f, "{seg}")?;
            first = false;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmpOp::Eq => write!(f, "=="),
            CmpOp::Ne => write!(f, "!="),
            CmpOp::Gt => write!(f, ">"),
            CmpOp::Lt => write!(f, "<"),
            CmpOp::Ge => write!(f, ">="),
            CmpOp::Le => write!(f, "<="),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// field.path == value
    Compare {
        left: FieldPath,
        op: CmpOp,
        right: MclValue,
    },
    /// field.path (truthy check)
    Truthy(FieldPath),
    /// !field.path (falsy check)
    Falsy(FieldPath),
    /// All conditions must be true
    All(Vec<Condition>),
    /// At least one condition must be true
    Any(Vec<Condition>),
}

impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::Compare { left, op, right } => write!(f, "{left} {op} {right}"),
            Condition::Truthy(path) => write!(f, "{path}"),
            Condition::Falsy(path) => write!(f, "!{path}"),
            Condition::All(conds) => {
                for (i, c) in conds.iter().enumerate() {
                    if i > 0 {
                        write!(f, " && ")?;
                    }
                    write!(f, "{c}")?;
                }
                Ok(())
            }
            Condition::Any(conds) => {
                for (i, c) in conds.iter().enumerate() {
                    if i > 0 {
                        write!(f, " || ")?;
                    }
                    write!(f, "{c}")?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_path_display() {
        let path = FieldPath::new(vec!["player".into(), "hp".into()]);
        assert_eq!(path.to_string(), "player.hp");
    }

    #[test]
    fn field_path_single() {
        let path = FieldPath::single("name");
        assert_eq!(path.to_string(), "name");
        assert_eq!(path.segments(), &["name".to_string()]);
    }

    #[test]
    fn cmp_op_display() {
        assert_eq!(CmpOp::Eq.to_string(), "==");
        assert_eq!(CmpOp::Ne.to_string(), "!=");
        assert_eq!(CmpOp::Gt.to_string(), ">");
        assert_eq!(CmpOp::Lt.to_string(), "<");
        assert_eq!(CmpOp::Ge.to_string(), ">=");
        assert_eq!(CmpOp::Le.to_string(), "<=");
    }

    #[test]
    fn condition_compare_display() {
        let cond = Condition::Compare {
            left: FieldPath::single("hp"),
            op: CmpOp::Gt,
            right: MclValue::Int(0),
        };
        assert_eq!(cond.to_string(), "hp > 0");
    }

    #[test]
    fn condition_truthy_falsy_display() {
        let t = Condition::Truthy(FieldPath::single("alive"));
        assert_eq!(t.to_string(), "alive");
        let f = Condition::Falsy(FieldPath::single("dead"));
        assert_eq!(f.to_string(), "!dead");
    }

    #[test]
    fn condition_all_display() {
        let all = Condition::All(vec![
            Condition::Truthy(FieldPath::single("a")),
            Condition::Compare {
                left: FieldPath::single("b"),
                op: CmpOp::Eq,
                right: MclValue::Int(1),
            },
        ]);
        assert_eq!(all.to_string(), "a && b == 1");
    }

    #[test]
    fn condition_any_display() {
        let any = Condition::Any(vec![
            Condition::Truthy(FieldPath::single("x")),
            Condition::Truthy(FieldPath::single("y")),
        ]);
        assert_eq!(any.to_string(), "x || y");
    }

    #[test]
    fn field_path_serde_round_trip() {
        let path = FieldPath::new(vec!["a".into(), "b".into(), "c".into()]);
        let json = serde_json::to_string(&path).unwrap();
        let back: FieldPath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, back);
    }
}
