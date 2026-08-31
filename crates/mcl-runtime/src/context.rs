use std::collections::BTreeMap;
use std::path::Path;

use indexmap::IndexMap;
use mcl_spec::document::{MclDocument, NarratorDef};
use mcl_spec::label::Lattice;
use mcl_spec::MclError;

use crate::state::WorldState;

/// Merge a base (parent) and child MCL document into a single document.
///
/// Shallow merge strategy:
/// - Child fields override parent fields where both are present
/// - Parent fields not in child are inherited
/// - Lists (transitions, knowledge, constraints) are merged (parent first, then child)
/// - IndexMap fields (state, entities, items, actions) merge by key: child wins per-key
/// - NarratorDef fields merge individually: child's present fields override parent's
/// - Frontmatter: child's frontmatter is kept (including context name)
pub fn merge_contexts(base: &MclDocument, child: &MclDocument) -> MclDocument {
    MclDocument {
        frontmatter: child.frontmatter.clone(),
        state: merge_indexmap(&base.state, &child.state),
        entities: merge_indexmap(&base.entities, &child.entities),
        items: merge_indexmap(&base.items, &child.items),
        actions: merge_indexmap(&base.actions, &child.actions),
        transitions: merge_vec(&base.transitions, &child.transitions),
        narrator: merge_narrator(&base.narrator, &child.narrator),
        narrator_constraints: child.narrator_constraints.clone(),
        knowledge: merge_vec(&base.knowledge, &child.knowledge),
        constraints: merge_vec(&base.constraints, &child.constraints),
        sources: merge_btreemap(&base.sources, &child.sources),
        labels: merge_labels(&base.labels, &child.labels),
        degrade_rules: merge_vec(&base.degrade_rules, &child.degrade_rules),
        policies: merge_indexmap(&base.policies, &child.policies),
        effects: merge_vec(&base.effects, &child.effects),
        span: child.span,
    }
}

fn merge_indexmap<V: Clone>(
    base: &IndexMap<String, V>,
    child: &IndexMap<String, V>,
) -> IndexMap<String, V> {
    let mut merged = base.clone();
    for (key, val) in child {
        merged.insert(key.clone(), val.clone());
    }
    merged
}

fn merge_vec<T: Clone>(base: &[T], child: &[T]) -> Vec<T> {
    let mut merged = base.to_vec();
    merged.extend_from_slice(child);
    merged
}

fn merge_narrator(base: &NarratorDef, child: &NarratorDef) -> NarratorDef {
    NarratorDef {
        tone: child.tone.clone().or(base.tone.clone()),
        voice: child.voice.clone().or(base.voice.clone()),
        tense: child.tense.clone().or(base.tense.clone()),
        context: child.context.clone().or(base.context.clone()),
        style_notes: child.style_notes.clone().or(base.style_notes.clone()),
        forbidden: merge_vec(&base.forbidden, &child.forbidden),
    }
}

fn merge_btreemap<K: Clone + Ord, V: Clone>(
    base: &BTreeMap<K, V>,
    child: &BTreeMap<K, V>,
) -> BTreeMap<K, V> {
    let mut merged = base.clone();
    for (key, val) in child {
        merged.insert(key.clone(), val.clone());
    }
    merged
}

fn merge_labels(base: &Lattice, child: &Lattice) -> Lattice {
    let mut labels = base.labels.clone();
    labels.extend_from_slice(&child.labels);
    Lattice { labels }
}

/// Manages loaded contexts and tracks the active one
pub struct ContextManager {
    /// All loaded contexts, keyed by context ID
    contexts: IndexMap<String, MclDocument>,
    /// Current active context ID
    active: Option<String>,
    /// Context stack for nested contexts (e.g. dialogue within room)
    stack: Vec<String>,
}

impl ContextManager {
    pub fn new() -> Self {
        Self {
            contexts: IndexMap::new(),
            active: None,
            stack: Vec::new(),
        }
    }

    /// Load a single MCL file
    pub fn load_file(&mut self, path: &Path) -> Result<(), MclError> {
        let doc = mcl_parse::parse_file(path)?;
        let id = doc.frontmatter.context.clone();
        self.contexts.insert(id, doc);
        Ok(())
    }

    /// Load a parsed document into the context manager
    pub fn load_document(&mut self, doc: MclDocument) {
        let id = doc.frontmatter.context.clone();
        self.contexts.insert(id, doc);
    }

    /// Load a document and resolve its `extends` field against already-loaded contexts.
    /// If the document's frontmatter has `extends`, looks up the parent by name and merges.
    pub fn load_document_with_inheritance(&mut self, doc: MclDocument) -> Result<(), MclError> {
        let extends = doc.frontmatter.extends.clone();
        let id = doc.frontmatter.context.clone();

        let merged = if let Some(parent_id) = extends {
            let parent = self
                .contexts
                .get(&parent_id)
                .ok_or(MclError::ContextNotFound(parent_id))?;
            merge_contexts(parent, &doc)
        } else {
            doc
        };

        self.contexts.insert(id, merged);
        Ok(())
    }

    /// Load all .mcl files from a directory
    pub fn load_directory(&mut self, dir: &Path) -> Result<(), MclError> {
        if !dir.is_dir() {
            return Err(MclError::Io(format!(
                "{} is not a directory",
                dir.display()
            )));
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "mcl"))
            .collect();

        entries.sort_by_key(|e| e.path());

        for entry in entries {
            self.load_file(&entry.path())?;
        }

        Ok(())
    }

    /// Set the active context and initialize state
    pub fn activate(&mut self, context_id: &str, state: &mut WorldState) -> Result<(), MclError> {
        let doc = self
            .contexts
            .get(context_id)
            .ok_or_else(|| MclError::ContextNotFound(context_id.to_string()))?;

        state.load_context_state(&doc.state);
        state.load_entities(&doc.entities);
        self.active = Some(context_id.to_string());

        Ok(())
    }

    /// Transition to a new context
    pub fn transition(&mut self, target: &str, state: &mut WorldState) -> Result<(), MclError> {
        if !self.contexts.contains_key(target) {
            return Err(MclError::ContextNotFound(target.to_string()));
        }

        // Push current to stack if we might come back
        if let Some(ref current) = self.active {
            self.stack.push(current.clone());
        }

        self.activate(target, state)
    }

    /// Pop back to the previous context
    pub fn pop(&mut self, state: &mut WorldState) -> Result<bool, MclError> {
        if let Some(prev) = self.stack.pop() {
            self.activate(&prev, state)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the active context document
    pub fn active_doc(&self) -> Option<&MclDocument> {
        self.active.as_ref().and_then(|id| self.contexts.get(id))
    }

    /// Get active context ID
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// Get a context by ID
    pub fn get(&self, id: &str) -> Option<&MclDocument> {
        self.contexts.get(id)
    }

    /// List all loaded context IDs
    pub fn context_ids(&self) -> Vec<&str> {
        self.contexts.keys().map(|s| s.as_str()).collect()
    }

    /// Check auto-transitions: returns target if any auto-transition condition is met
    pub fn check_auto_transitions(&self, state: &WorldState) -> Option<String> {
        let doc = self.active_doc()?;
        for transition in &doc.transitions {
            if !transition.auto {
                continue;
            }
            if let Some(ref cond) = transition.when {
                if state.evaluate(cond) {
                    return Some(transition.target.clone());
                }
            }
        }
        None
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> MclDocument {
        mcl_parse::parse_document(src).unwrap()
    }

    #[test]
    fn extended_context_inherits_state() {
        let base = parse("---\ncontext: base\ntype: room\n---\n# State\nhp: 100\n");
        let child = parse("---\ncontext: child\ntype: room\nextends: base\n---\n# State\nmp: 50\n");
        let merged = merge_contexts(&base, &child);
        assert_eq!(merged.state.get("hp").unwrap().as_int(), Some(100));
        assert_eq!(merged.state.get("mp").unwrap().as_int(), Some(50));
    }

    #[test]
    fn child_overrides_parent_state() {
        let base = parse("---\ncontext: base\ntype: room\n---\n# State\nhp: 100\n");
        let child = parse("---\ncontext: child\ntype: room\nextends: base\n---\n# State\nhp: 50\n");
        let merged = merge_contexts(&base, &child);
        assert_eq!(merged.state.get("hp").unwrap().as_int(), Some(50));
    }

    #[test]
    fn child_inherits_entities() {
        let base = parse("---\ncontext: base\ntype: room\n---\n# Entities\n- goblin:\n    type: enemy\n    hp: 12\n");
        let child = parse("---\ncontext: child\ntype: room\nextends: base\n---\n");
        let merged = merge_contexts(&base, &child);
        assert!(merged.entities.contains_key("goblin"));
    }

    #[test]
    fn child_overrides_narrator() {
        let base = parse("---\ncontext: base\ntype: room\n---\n# Narrator\ntone: dread\n");
        let child = parse(
            "---\ncontext: child\ntype: room\nextends: base\n---\n# Narrator\ntone: cheerful\n",
        );
        let merged = merge_contexts(&base, &child);
        assert_eq!(merged.narrator.tone.as_deref(), Some("cheerful"));
    }

    #[test]
    fn child_inherits_transitions() {
        let base = parse("---\ncontext: base\ntype: room\n---\n# Transitions\n- next_room:\n    when: door_open\n");
        let child = parse("---\ncontext: child\ntype: room\nextends: base\n---\n");
        let merged = merge_contexts(&base, &child);
        assert_eq!(merged.transitions.len(), 1);
        assert_eq!(merged.transitions[0].target, "next_room");
    }

    #[test]
    fn merge_without_extends_returns_child() {
        let child = parse("---\ncontext: child\ntype: room\n---\n# State\nmp: int = 50\n");
        let base = parse("---\ncontext: base\ntype: room\n---\n# State\nhp: int = 100\n");
        let merged = merge_contexts(&base, &child);
        assert_eq!(merged.frontmatter.context, "child");
        assert!(merged.state.contains_key("mp"));
        assert!(merged.state.contains_key("hp"));
    }
}
