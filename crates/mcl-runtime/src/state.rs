use indexmap::IndexMap;
use mcl_spec::expression::{CmpOp, Condition, FieldPath};
use mcl_spec::value::MclValue;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};

use crate::audit::AuditLog;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub global: IndexMap<String, MclValue>,
    pub local: IndexMap<String, MclValue>,
    pub entities: IndexMap<String, IndexMap<String, MclValue>>,
    pub player: IndexMap<String, MclValue>,
    pub changelog: Vec<StateChange>,
    #[serde(skip)]
    audit_log: Option<AuditLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub path: String,
    pub old: MclValue,
    pub new: MclValue,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            global: IndexMap::new(),
            local: IndexMap::new(),
            entities: IndexMap::new(),
            player: IndexMap::new(),
            changelog: Vec::new(),
            audit_log: None,
        }
    }

    pub fn set_audit_log(&mut self, log: AuditLog) {
        self.audit_log = Some(log);
    }

    pub fn load_context_state(&mut self, state: &IndexMap<String, MclValue>) {
        self.local = state.clone();
    }

    /// Load entities from a document
    pub fn load_entities(&mut self, entities: &IndexMap<String, mcl_spec::EntityDef>) {
        self.entities.clear();
        for (id, def) in entities {
            let mut props = def.properties.clone();
            if let Some(ref t) = def.entity_type {
                props.insert("type".to_string(), MclValue::String(t.clone()));
            }
            if let Some(ref d) = def.disposition {
                props.insert("disposition".to_string(), MclValue::String(d.clone()));
            }
            self.entities.insert(id.clone(), props);
        }
    }

    /// Get a value by field path
    pub fn get(&self, path: &FieldPath) -> Option<&MclValue> {
        let segs = path.segments();
        if segs.is_empty() {
            return None;
        }

        match segs[0].as_str() {
            "player" => navigate_map(&self.player, &segs[1..]),
            "global" => navigate_map(&self.global, &segs[1..]),
            "entities" if segs.len() >= 2 => {
                let entity = self.entities.get(&segs[1])?;
                if segs.len() == 2 {
                    return None; // can't return the whole entity map as MclValue
                }
                navigate_map(entity, &segs[2..])
            }
            _ => {
                // Try local state first, then global
                if let Some(val) = navigate_map(&self.local, segs) {
                    return Some(val);
                }
                navigate_map(&self.global, segs)
            }
        }
    }

    pub fn set(&mut self, path_str: &str, value: MclValue) -> Result<(), MclError> {
        let parts: Vec<&str> = path_str.split('.').collect();
        if parts.is_empty() {
            return Err(MclError::State("empty path".to_string()));
        }

        let old = self.get_by_str(path_str).cloned().unwrap_or(MclValue::Null);

        match parts[0] {
            "player" if parts.len() >= 2 => {
                self.player.insert(parts[1].to_string(), value.clone());
            }
            "global" if parts.len() >= 2 => {
                self.global.insert(parts[1].to_string(), value.clone());
            }
            "entities" if parts.len() >= 3 => {
                let entity = self.entities.entry(parts[1].to_string()).or_default();
                entity.insert(parts[2].to_string(), value.clone());
            }
            _ => {
                self.local.insert(parts[0].to_string(), value.clone());
            }
        }

        if let Some(ref mut log) = self.audit_log {
            log.record(path_str, old.clone(), value.clone())?;
        }

        self.changelog.push(StateChange {
            path: path_str.to_string(),
            old,
            new: value,
        });

        Ok(())
    }

    /// Get by dot-separated string path
    pub fn get_by_str(&self, path: &str) -> Option<&MclValue> {
        let parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
        self.get(&FieldPath::new(parts))
    }

    /// Evaluate a condition against current state
    pub fn evaluate(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Truthy(path) => self.get(path).is_some_and(|v| v.is_truthy()),
            Condition::Falsy(path) => self.get(path).is_none_or(|v| !v.is_truthy()),
            Condition::Compare { left, op, right } => {
                let val = match self.get(left) {
                    Some(v) => v,
                    None => return false,
                };
                compare_values(val, *op, right)
            }
            Condition::All(conds) => conds.iter().all(|c| self.evaluate(c)),
            Condition::Any(conds) => conds.iter().any(|c| self.evaluate(c)),
        }
    }

    /// Clear turn changelog
    pub fn clear_changelog(&mut self) {
        self.changelog.clear();
    }

    /// Apply a map of mutations (from action.mutates)
    pub fn apply_mutations(
        &mut self,
        mutations: &IndexMap<String, MclValue>,
    ) -> Result<(), MclError> {
        for (path, value) in mutations {
            self.set(path, value.clone())?;
        }
        Ok(())
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

fn navigate_map<'a>(map: &'a IndexMap<String, MclValue>, path: &[String]) -> Option<&'a MclValue> {
    if path.is_empty() {
        return None;
    }
    let val = map.get(&path[0])?;
    if path.len() == 1 {
        return Some(val);
    }
    // Recurse into nested maps
    if let MclValue::Map(inner) = val {
        navigate_map(inner, &path[1..])
    } else {
        None
    }
}

fn compare_values(left: &MclValue, op: CmpOp, right: &MclValue) -> bool {
    match op {
        CmpOp::Eq => values_equal(left, right),
        CmpOp::Ne => !values_equal(left, right),
        CmpOp::Gt => numeric_cmp(left, right) == Some(std::cmp::Ordering::Greater),
        CmpOp::Lt => numeric_cmp(left, right) == Some(std::cmp::Ordering::Less),
        CmpOp::Ge => numeric_cmp(left, right).is_some_and(|ord| ord != std::cmp::Ordering::Less),
        CmpOp::Le => numeric_cmp(left, right).is_some_and(|ord| ord != std::cmp::Ordering::Greater),
    }
}

fn values_equal(a: &MclValue, b: &MclValue) -> bool {
    match (a, b) {
        (MclValue::String(a), MclValue::String(b)) => a == b,
        (MclValue::Int(a), MclValue::Int(b)) => a == b,
        (MclValue::Float(a), MclValue::Float(b)) => a == b,
        (MclValue::Bool(a), MclValue::Bool(b)) => a == b,
        (MclValue::Null, MclValue::Null) => true,
        (MclValue::Int(a), MclValue::Float(b)) => (*a as f64) == *b,
        (MclValue::Float(a), MclValue::Int(b)) => *a == (*b as f64),
        _ => false,
    }
}

fn numeric_cmp(a: &MclValue, b: &MclValue) -> Option<std::cmp::Ordering> {
    let a_f = a.as_float()?;
    let b_f = b.as_float()?;
    a_f.partial_cmp(&b_f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_local() {
        let mut state = WorldState::new();
        state.set("lit", MclValue::Bool(false)).unwrap();
        assert_eq!(state.get_by_str("lit"), Some(&MclValue::Bool(false)));

        state.set("lit", MclValue::Bool(true)).unwrap();
        assert_eq!(state.get_by_str("lit"), Some(&MclValue::Bool(true)));
        assert_eq!(state.changelog.len(), 2);
    }

    #[test]
    fn get_set_player() {
        let mut state = WorldState::new();
        state.set("player.hp", MclValue::Int(100)).unwrap();
        assert_eq!(
            state.get(&FieldPath::new(vec!["player".into(), "hp".into()])),
            Some(&MclValue::Int(100))
        );
    }

    #[test]
    fn evaluate_truthy() {
        let mut state = WorldState::new();
        state.set("lit", MclValue::Bool(true)).unwrap();
        assert!(state.evaluate(&Condition::Truthy(FieldPath::single("lit"))));

        state.set("lit", MclValue::Bool(false)).unwrap();
        assert!(!state.evaluate(&Condition::Truthy(FieldPath::single("lit"))));
    }

    #[test]
    fn evaluate_compare() {
        let mut state = WorldState::new();
        state
            .set("door", MclValue::String("unlocked".into()))
            .unwrap();

        let cond = Condition::Compare {
            left: FieldPath::single("door"),
            op: CmpOp::Eq,
            right: MclValue::String("unlocked".into()),
        };
        assert!(state.evaluate(&cond));

        let cond_ne = Condition::Compare {
            left: FieldPath::single("door"),
            op: CmpOp::Eq,
            right: MclValue::String("locked".into()),
        };
        assert!(!state.evaluate(&cond_ne));
    }

    #[test]
    fn evaluate_numeric_compare() {
        let mut state = WorldState::new();
        state.set("player.hp", MclValue::Int(50)).unwrap();

        let cond = Condition::Compare {
            left: FieldPath::new(vec!["player".into(), "hp".into()]),
            op: CmpOp::Gt,
            right: MclValue::Int(0),
        };
        assert!(state.evaluate(&cond));
    }

    #[test]
    fn serde_round_trip_all_value_types() {
        let mut state = WorldState::new();
        state.set("int_val", MclValue::Int(42)).unwrap();
        state.set("float_val", MclValue::Float(3.14)).unwrap();
        state
            .set("string_val", MclValue::String("hello".into()))
            .unwrap();
        state.set("bool_val", MclValue::Bool(true)).unwrap();
        state.set("null_val", MclValue::Null).unwrap();
        state
            .set(
                "list_val",
                MclValue::List(vec![MclValue::Int(1), MclValue::Int(2)]),
            )
            .unwrap();

        let mut map = IndexMap::new();
        map.insert("key".to_string(), MclValue::String("value".into()));
        state.set("map_val", MclValue::Map(map)).unwrap();

        let json = serde_json::to_string(&state).expect("serialize WorldState");
        let back: WorldState = serde_json::from_str(&json).expect("deserialize WorldState");

        assert_eq!(state.local, back.local);
        assert_eq!(state.global, back.global);
        assert_eq!(state.entities, back.entities);
        assert_eq!(state.player, back.player);
    }

    #[test]
    fn serde_round_trip_preserves_changelog() {
        let mut state = WorldState::new();
        state.set("hp", MclValue::Int(100)).unwrap();
        state.set("hp", MclValue::Int(50)).unwrap();

        assert_eq!(state.changelog.len(), 2);

        let json = serde_json::to_string(&state).expect("serialize WorldState");
        let back: WorldState = serde_json::from_str(&json).expect("deserialize WorldState");

        assert_eq!(back.changelog.len(), 2);
        assert_eq!(back.changelog[0].path, "hp");
        assert_eq!(back.changelog[0].old, MclValue::Null);
        assert_eq!(back.changelog[0].new, MclValue::Int(100));
        assert_eq!(back.changelog[1].old, MclValue::Int(100));
        assert_eq!(back.changelog[1].new, MclValue::Int(50));
    }

    #[test]
    fn serde_round_trip_entities_player_global() {
        let mut state = WorldState::new();
        state.set("player.hp", MclValue::Int(80)).unwrap();
        state
            .set("player.name", MclValue::String("Hero".into()))
            .unwrap();
        state.set("global.turn_count", MclValue::Int(5)).unwrap();

        let mut entity_props = IndexMap::new();
        entity_props.insert("hp".to_string(), MclValue::Int(20));
        entity_props.insert("type".to_string(), MclValue::String("enemy".into()));
        state.entities.insert("goblin".to_string(), entity_props);

        state.set("found_key", MclValue::Bool(true)).unwrap();

        let json = serde_json::to_string(&state).expect("serialize");
        let back: WorldState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.player.get("hp"), Some(&MclValue::Int(80)));
        assert_eq!(
            back.player.get("name"),
            Some(&MclValue::String("Hero".into()))
        );
        assert_eq!(back.global.get("turn_count"), Some(&MclValue::Int(5)));
        assert_eq!(back.local.get("found_key"), Some(&MclValue::Bool(true)));
        assert_eq!(
            back.entities.get("goblin").unwrap().get("hp"),
            Some(&MclValue::Int(20))
        );
    }
}
