use indexmap::IndexMap;
use mcl_spec::document::{MclDocument, NarratorDef};
use mcl_spec::label::Lattice;
use mcl_spec::policy::PolicyDef;
use mcl_spec::source::SourceConfig;
use mcl_spec::{MclError, Span};

use crate::blocks;
use crate::frontmatter::parse_frontmatter;
use crate::lexer::{lex, Line, LineKind};

/// Parse an MCL document from source text
pub fn parse_document(source: &str) -> Result<MclDocument, MclError> {
    let lines = lex(source);
    if lines.is_empty() {
        return Err(MclError::parse("empty document", Span::empty()));
    }

    let doc_span = lines[0].span;

    // Parse frontmatter
    let (frontmatter, mut pos) = parse_frontmatter(&lines, 0)?;

    // Parse sections
    let mut state = IndexMap::new();
    let mut entities = IndexMap::new();
    let mut items = IndexMap::new();
    let mut actions = IndexMap::new();
    let mut transitions = Vec::new();
    let mut narrator = NarratorDef::default();
    let mut narrator_constraints = None;
    let mut knowledge = Vec::new();
    let mut constraints = Vec::new();
    let mut sources = SourceConfig::new();
    let mut labels = Lattice { labels: Vec::new() };
    let mut degrade_rules = Vec::new();
    let mut policies: IndexMap<String, PolicyDef> = IndexMap::new();

    while pos < lines.len() {
        match &lines[pos].kind {
            LineKind::SectionHeader(name) => {
                pos += 1;
                let section_lines = collect_section(&lines, pos);
                let section_len = section_lines.len();

                match name.as_str() {
                    "State" => state = blocks::parse_state(section_lines)?,
                    "Entities" => entities = blocks::parse_entities(section_lines)?,
                    "Items" => items = blocks::parse_items(section_lines)?,
                    "Actions" => actions = blocks::parse_actions(section_lines)?,
                    "Transitions" => transitions = blocks::parse_transitions(section_lines)?,
                    "Narrator" => {
                        let (n, nc) = blocks::parse_narrator(section_lines)?;
                        narrator = n;
                        narrator_constraints = nc;
                    }
                    "Knowledge" => knowledge = blocks::parse_knowledge(section_lines)?,
                    "Constraints" => constraints = blocks::parse_constraints(section_lines)?,
                    "Sources" => sources = blocks::parse_sources(section_lines)?,
                    "Labels" => labels = blocks::parse_labels(section_lines)?,
                    "Degrade" => degrade_rules = blocks::parse_degrade(section_lines)?,
                    "Policies" => policies = crate::policy::parse_policies(section_lines)?,
                    "Includes" => {
                        // TODO: implement includes in Phase 2
                    }
                    _ => {
                        log::warn!("unknown section `{name}`, skipping");
                    }
                }

                pos += section_len;
            }
            LineKind::Blank | LineKind::Comment(_) => {
                pos += 1;
            }
            _ => {
                pos += 1;
            }
        }
    }

    Ok(MclDocument {
        frontmatter,
        state,
        entities,
        items,
        actions,
        transitions,
        narrator,
        narrator_constraints,
        knowledge,
        constraints,
        sources,
        labels,
        degrade_rules,
        policies,
        effects: Vec::new(),
        span: doc_span,
    })
}

/// Parse an MCL document from a file path
pub fn parse_file(path: &std::path::Path) -> Result<MclDocument, MclError> {
    let source = std::fs::read_to_string(path)?;
    parse_document(&source)
}

/// Collect lines belonging to a section (until next section header or EOF)
fn collect_section(lines: &[Line], start: usize) -> &[Line] {
    let mut end = start;
    while end < lines.len() {
        if matches!(&lines[end].kind, LineKind::SectionHeader(_)) {
            break;
        }
        end += 1;
    }
    &lines[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::document::{ContextType, KnowledgeEntry, TargetSpec};

    const SAMPLE_DOC: &str = r#"---
context: crypt_antechamber
type: room
domain: rpg
---

# State
lit: false
door_north: locked

# Entities
- skeleton_guard:
    type: enemy
    disposition: hostile
    hp: 12
    knowledge: [guards_tomb, knows_nothing_of_key]

# Items
- rusty_key: { on: floor, visible_when: lit }
- torch_sconce: { on: wall, interactive: true }

# Actions
- light_torch:
    requires: [inventory.flint]
    mutates: { lit: true }
- attack:
    target: enum(entities.enemies)
    resolution: combat_system
- unlock_door:
    target: door_north
    requires: [inventory.rusty_key]
    mutates: { door_north: unlocked }
- speak:
    target: enum(entities)
    mode: dialogue

# Transitions
- crypt_inner:
    when: door_north == unlocked
    direction: north

# Narrator
tone: dread
voice: second_person
tense: present
context: "Ancient crypt, smell of dust and decay. The skeleton animates as the player enters."

# Knowledge
- guards_tomb
- !secret_passage
"#;

    #[test]
    fn parse_full_document() {
        let doc = parse_document(SAMPLE_DOC).unwrap();

        // Frontmatter
        assert_eq!(doc.frontmatter.context, "crypt_antechamber");
        assert_eq!(doc.frontmatter.context_type, ContextType::Room);
        assert_eq!(doc.frontmatter.domain, Some("rpg".to_string()));

        // State
        assert_eq!(doc.state.len(), 2);
        assert_eq!(doc.state.get("lit").unwrap().as_bool(), Some(false));

        // Entities
        assert_eq!(doc.entities.len(), 1);
        let skel = &doc.entities["skeleton_guard"];
        assert_eq!(skel.entity_type, Some("enemy".to_string()));
        assert_eq!(skel.disposition, Some("hostile".to_string()));
        assert_eq!(skel.knowledge.len(), 2);

        // Items
        assert_eq!(doc.items.len(), 2);
        assert!(doc.items.contains_key("rusty_key"));

        // Actions
        assert_eq!(doc.actions.len(), 4);
        assert!(!doc.actions["light_torch"].requires.is_empty());
        assert_eq!(
            doc.actions["attack"].resolution,
            Some("combat_system".to_string())
        );
        assert!(doc.actions["unlock_door"]
            .mutates
            .contains_key("door_north"));

        // Transitions
        assert_eq!(doc.transitions.len(), 1);
        assert_eq!(doc.transitions[0].target, "crypt_inner");
        assert_eq!(doc.transitions[0].direction, Some("north".to_string()));

        // Narrator
        assert_eq!(doc.narrator.tone, Some("dread".to_string()));
        assert_eq!(doc.narrator.voice, Some("second_person".to_string()));
        assert!(doc.narrator.context.is_some());

        // Knowledge
        assert_eq!(doc.knowledge.len(), 2);
        assert!(matches!(&doc.knowledge[0], KnowledgeEntry::Known(s) if s == "guards_tomb"));
        assert!(matches!(&doc.knowledge[1], KnowledgeEntry::Unknown(s) if s == "secret_passage"));
    }

    /// Reference: Frontmatter section - all field types
    #[test]
    fn langref_frontmatter_all_fields() {
        let src = r#"---
context: market_square
type: scene
domain: rpg
version: 2
tags: [town, daytime]
extends: base_location
---
"#;
        let doc = parse_document(src).unwrap();
        let fm = &doc.frontmatter;
        assert_eq!(fm.context, "market_square");
        assert_eq!(fm.context_type, ContextType::Scene);
        assert_eq!(fm.domain, Some("rpg".to_string()));
        assert_eq!(fm.version, Some(2));
        assert_eq!(fm.tags, vec!["town", "daytime"]);
        assert_eq!(fm.extends, Some("base_location".to_string()));
    }

    /// Reference: State section - all value types
    #[test]
    fn langref_state_all_value_types() {
        let src = r#"---
context: test_state
type: room
---

# State
lit: false
door_north: locked
torch_count: 3
temperature: 0.7
label: "ancient crypt"
features: [dark, damp]
metadata: { owner: skeleton_king, age: 500 }
empty: null
tilde_null: ~
"#;
        let doc = parse_document(src).unwrap();
        let s = &doc.state;

        assert_eq!(s.get("lit").unwrap().as_bool(), Some(false));
        assert_eq!(s.get("torch_count").unwrap().as_int(), Some(3));
        assert!(matches!(
            s.get("temperature").unwrap(),
            mcl_spec::MclValue::Float(_)
        ));
        assert_eq!(s.get("label").unwrap().as_str(), Some("ancient crypt"));
        assert_eq!(s.get("door_north").unwrap().as_str(), Some("locked"));
        assert!(s.get("features").unwrap().as_list().is_some());
        assert!(s.get("metadata").unwrap().as_map().is_some());
        assert!(matches!(s.get("empty").unwrap(), mcl_spec::MclValue::Null));
        assert!(matches!(
            s.get("tilde_null").unwrap(),
            mcl_spec::MclValue::Null
        ));
    }

    /// Reference: Entities section - inline and indented properties
    #[test]
    fn langref_entities_inline_and_indented() {
        let src = r#"---
context: test_entities
type: room
---

# Entities
- skeleton_guard:
    type: enemy
    disposition: hostile
    hp: 12
    ac: 12
    knowledge: [guards_tomb, knows_nothing_of_key]
- ghost_king: { type: boss, hp: 50 }
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.entities.len(), 2);

        let skel = &doc.entities["skeleton_guard"];
        assert_eq!(skel.entity_type, Some("enemy".to_string()));
        assert_eq!(skel.disposition, Some("hostile".to_string()));
        assert_eq!(skel.knowledge, vec!["guards_tomb", "knows_nothing_of_key"]);

        let ghost = &doc.entities["ghost_king"];
        assert_eq!(ghost.entity_type, Some("boss".to_string()));
        assert_eq!(ghost.properties.get("hp").unwrap().as_int(), Some(50));
    }

    /// Reference: Items section - inline and indented
    #[test]
    fn langref_items_inline_and_indented() {
        let src = r#"---
context: test_items
type: room
---

# Items
- rusty_key: { on: floor, visible_when: lit }
- torch_sconce: { on: wall, interactive: true }
- magic_sword:
    damage: 10
    enchantment: fire
    weight: 3.5
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.items.len(), 3);

        let key = &doc.items["rusty_key"];
        assert_eq!(key.properties.get("on").unwrap().as_str(), Some("floor"));
        assert_eq!(
            key.properties.get("visible_when").unwrap().as_str(),
            Some("lit")
        );

        let sword = &doc.items["magic_sword"];
        assert_eq!(sword.properties.get("damage").unwrap().as_int(), Some(10));
        assert!(matches!(
            sword.properties.get("weight").unwrap(),
            mcl_spec::MclValue::Float(_)
        ));
    }

    /// Reference: Actions section - all field types
    #[test]
    fn langref_actions_full() {
        let src = r#"---
context: test_actions
type: room
---

# Actions
- light_torch:
    requires: [inventory.flint]
    mutates: { lit: true }
    description: "Light the torch sconce on the wall"
- attack:
    target: enum(entities.enemies)
    resolution: combat_system
    description: "Attack an enemy"
- unlock_door:
    target: door_north
    requires: [inventory.rusty_key]
    mutates: { door_north: unlocked }
    description: "Unlock the northern door with a key"
- speak:
    target: enum(entities)
    mode: dialogue
    description: "Attempt to speak with someone"
- look:
    description: "Look around the room"
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.actions.len(), 5);

        let light = &doc.actions["light_torch"];
        assert!(!light.requires.is_empty());
        assert!(light.mutates.contains_key("lit"));
        assert_eq!(
            light.description,
            Some("Light the torch sconce on the wall".to_string())
        );

        let attack = &doc.actions["attack"];
        assert!(matches!(attack.target, Some(TargetSpec::Enum(_))));
        assert_eq!(attack.resolution, Some("combat_system".to_string()));

        let unlock = &doc.actions["unlock_door"];
        match &unlock.target {
            Some(TargetSpec::Fixed(name)) => assert_eq!(name, "door_north"),
            other => panic!("expected Fixed(\"door_north\"), got {other:?}"),
        }
        assert!(unlock.mutates.contains_key("door_north"));

        let speak = &doc.actions["speak"];
        assert_eq!(speak.mode, Some("dialogue".to_string()));
    }

    /// Reference: Actions section - cooldown and args
    #[test]
    fn langref_actions_cooldown_and_args() {
        let src = r#"---
context: test_cooldown
type: room
---

# Actions
- heal:
    cooldown: 3
    args: { cost: 10, target: self }
    description: "Heal yourself"
"#;
        let doc = parse_document(src).unwrap();
        let heal = &doc.actions["heal"];
        assert_eq!(heal.cooldown, Some(3));
        assert_eq!(heal.args.get("cost").unwrap(), "10");
    }

    /// Reference: Transitions section - with conditions, auto, narrate
    #[test]
    fn langref_transitions_full() {
        let src = r#"---
context: test_transitions
type: room
---

# Transitions
- crypt_inner:
    when: door_north == unlocked
    direction: north
- collapse:
    when: timer == 0
    auto: true
    narrate: "The ceiling gives way!"
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.transitions.len(), 2);

        let t1 = &doc.transitions[0];
        assert_eq!(t1.target, "crypt_inner");
        assert_eq!(t1.direction, Some("north".to_string()));
        assert!(t1.when.is_some());
        assert!(!t1.auto);

        let t2 = &doc.transitions[1];
        assert_eq!(t2.target, "collapse");
        assert!(t2.auto);
        assert_eq!(t2.narrate, Some("The ceiling gives way!".to_string()));
    }

    /// Reference: Narrator section - all fields
    #[test]
    fn langref_narrator_full() {
        let src = r#"---
context: test_narrator
type: room
---

# Narrator
tone: dread
voice: second_person
tense: present
context: "You stand at the entrance of an ancient crypt."
style_notes: "Keep descriptions atmospheric and tense."
forbidden: [epic, awesome, incredible]
"#;
        let doc = parse_document(src).unwrap();
        let n = &doc.narrator;
        assert_eq!(n.tone, Some("dread".to_string()));
        assert_eq!(n.voice, Some("second_person".to_string()));
        assert_eq!(n.tense, Some("present".to_string()));
        assert!(n.context.is_some());
        assert!(n.style_notes.is_some());
        assert_eq!(n.forbidden, vec!["epic", "awesome", "incredible"]);
    }

    /// Reference: Knowledge section - knows and ignores
    #[test]
    fn langref_knows_and_ignores() {
        let src = r#"---
context: test_knowledge
type: room
---

# Knowledge
- guards_tomb
- !secret_passage
- !treasure_room
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.knowledge.len(), 3);
        assert!(matches!(&doc.knowledge[0], KnowledgeEntry::Known(s) if s == "guards_tomb"));
        assert!(matches!(&doc.knowledge[1], KnowledgeEntry::Unknown(s) if s == "secret_passage"));
        assert!(matches!(&doc.knowledge[2], KnowledgeEntry::Unknown(s) if s == "treasure_room"));
    }

    /// Reference: Constraints section - free-form rules
    #[test]
    fn langref_constraints() {
        let src = r#"---
context: test_constraints
type: room
---

# Constraints
- The skeleton does not attack first unless the player approaches the door
- The rusty key is only visible when the room is lit
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.constraints.len(), 2);
        assert!(doc.constraints[0].contains("skeleton"));
        assert!(doc.constraints[1].contains("rusty key"));
    }

    /// Reference: All context types parse correctly
    #[test]
    fn langref_all_context_types() {
        for (type_str, expected) in [
            ("room", ContextType::Room),
            ("scene", ContextType::Scene),
            ("step", ContextType::Step),
            ("dialogue", ContextType::Dialogue),
            ("encounter", ContextType::Encounter),
        ] {
            let src = format!("---\ncontext: test_{type_str}\ntype: {type_str}\n---\n");
            let doc = parse_document(&src).unwrap();
            assert_eq!(
                doc.frontmatter.context_type, expected,
                "failed for type {type_str}"
            );
        }
    }

    /// Reference: Comments and blank lines are ignored
    #[test]
    fn langref_comments_and_blanks() {
        let src = r#"---
context: test_comments
type: room
---

// This is a full-line comment
# State
lit: false  // inline comment after value

// Another comment between sections
# Narrator
tone: calm
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.state.len(), 1);
        assert_eq!(doc.narrator.tone, Some("calm".to_string()));
    }

    /// Reference: The complete crypt_antechamber.mcl example from examples/
    #[test]
    fn langref_full_crypt_example() {
        let src = include_str!("../../../examples/crypt/crypt_antechamber.mcl");
        let doc = parse_document(src).unwrap();

        assert_eq!(doc.frontmatter.context, "crypt_antechamber");
        assert_eq!(doc.frontmatter.context_type, ContextType::Room);
        assert_eq!(doc.frontmatter.domain, Some("rpg".to_string()));

        assert_eq!(doc.state.len(), 2);
        assert_eq!(doc.state.get("lit").unwrap().as_bool(), Some(false));

        assert_eq!(doc.entities.len(), 1);
        let skel = &doc.entities["skeleton_guard"];
        assert_eq!(skel.entity_type, Some("enemy".to_string()));
        assert_eq!(skel.disposition, Some("hostile".to_string()));
        assert_eq!(skel.knowledge.len(), 2);

        assert_eq!(doc.items.len(), 3);

        assert_eq!(doc.actions.len(), 6);

        assert_eq!(doc.transitions.len(), 1);
        assert_eq!(doc.transitions[0].target, "crypt_inner");
        assert_eq!(doc.transitions[0].direction, Some("north".to_string()));

        assert_eq!(doc.narrator.tone, Some("dread".to_string()));
        assert_eq!(doc.narrator.voice, Some("second_person".to_string()));
        assert_eq!(doc.narrator.forbidden.len(), 3);

        assert_eq!(doc.knowledge.len(), 3);

        assert_eq!(doc.constraints.len(), 3);
    }

    #[test]
    fn parse_sources_block() {
        let src = r#"---
context: test_sources
type: room
---

# Sources
player_input: { trust: user }
rag_retrieval: { trust: untrusted, inherit_labels: [rag_origin] }
webhook_orders: { trust: external, verify: hmac_sha256(secret_env=ORDERS_HMAC) }
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.sources.len(), 3);

        let player = &doc.sources[&"player_input".into()];
        assert_eq!(player.trust, mcl_spec::source::Trust::User);

        let rag = &doc.sources[&"rag_retrieval".into()];
        assert_eq!(rag.trust, mcl_spec::source::Trust::Untrusted);
        assert_eq!(rag.inherit_labels, vec!["rag_origin"]);

        let webhook = &doc.sources[&"webhook_orders".into()];
        assert_eq!(webhook.trust, mcl_spec::source::Trust::External);
        assert!(webhook.verify.is_some());
    }

    #[test]
    fn parse_narrator_extensions() {
        let src = r#"---
context: test_nc
type: room
---

# Narrator
tone: formal
voice: first_person
must_not_mention: [@secret, @internal]
must_not_emit_pattern: ["\d{3}-\d{2}-\d{4}"]
must_not_emit_word: [forbidden_term]
prose_max_length: 500
on_violation: deny
"#;
        let doc = parse_document(src).unwrap();
        assert!(doc.narrator_constraints.is_some());
        let nc = doc.narrator_constraints.as_ref().unwrap();
        assert_eq!(nc.must_not_mention, vec!["@secret", "@internal"]);
        assert_eq!(nc.must_not_emit_pattern, vec![r"\d{3}-\d{2}-\d{4}"]);
        assert_eq!(nc.must_not_emit_word, vec!["forbidden_term"]);
        assert_eq!(nc.prose_max_length, Some(500));
        assert_eq!(nc.on_violation, mcl_spec::narrator::ViolationAction::Deny);
    }

    #[test]
    fn parse_forbidden_backward_compat() {
        let src = r#"---
context: test_compat
type: room
---

# Narrator
tone: dread
forbidden: [epic, awesome, incredible]
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(
            doc.narrator.forbidden,
            vec!["epic", "awesome", "incredible"]
        );
        assert!(doc.narrator_constraints.is_none());
    }

    #[test]
    fn forbidden_desugars_to_must_not_emit_word() {
        let src_forbidden = r#"---
context: test_forbidden
type: room
---

# Narrator
forbidden: [epic, awesome]
"#;
        let doc_forbidden = parse_document(src_forbidden).unwrap();

        let src_must_not = r#"---
context: test_must_not
type: room
---

# Narrator
must_not_emit_word: [epic, awesome]
"#;
        let doc_must_not = parse_document(src_must_not).unwrap();

        assert_eq!(doc_forbidden.narrator.forbidden, vec!["epic", "awesome"]);
        assert!(doc_forbidden.narrator_constraints.is_none());

        assert_eq!(
            doc_must_not
                .narrator_constraints
                .as_ref()
                .unwrap()
                .must_not_emit_word,
            vec!["epic", "awesome"]
        );
        assert!(doc_must_not.narrator.forbidden.is_empty());
    }

    #[test]
    fn parse_labels_lattice() {
        let src = r#"---
context: test_labels
type: room
---

# Labels
- phi: { dominates: [pii], default_egress: deny }
- secret: { params: [audience], scope: per_audience(buyer) }
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.labels.labels.len(), 2);

        let phi = &doc.labels.labels[0];
        assert_eq!(phi.name, "phi");
        assert_eq!(phi.dominates, vec!["pii"]);

        let secret = &doc.labels.labels[1];
        assert_eq!(secret.name, "secret");
        assert_eq!(secret.params, vec!["audience"]);
    }

    #[test]
    fn parse_degrade_block() {
        let src = r#"---
context: test_degrade
type: room
---

# Degrade
    degrade phi to clinical_only {
        redact: [name, ssn]
        preserve: [diagnosis_codes]
    }
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.degrade_rules.len(), 1);

        let rule = &doc.degrade_rules[0];
        assert_eq!(rule.label, "phi");
        assert_eq!(rule.to_level, "clinical_only");
        assert_eq!(rule.redact, vec!["name", "ssn"]);
        assert_eq!(rule.preserve, vec!["diagnosis_codes"]);
    }

    #[test]
    fn parse_existing_fixtures_unchanged() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixture_dirs = [
            base.join("examples/crypt"),
            base.join("examples/debate"),
            base.join("examples/multi_persona"),
            base.join("examples/quiz"),
        ];
        for dir in &fixture_dirs {
            let entry = std::fs::read_dir(dir)
                .unwrap_or_else(|_| panic!("fixture dir {:?} not found", dir));
            for file in entry {
                let file = file.unwrap();
                if file.path().extension().is_some_and(|e| e == "mcl") {
                    let source = std::fs::read_to_string(file.path()).unwrap();
                    assert!(
                        parse_document(&source).is_ok(),
                        "failed to parse fixture: {:?}",
                        file.path()
                    );
                }
            }
        }
    }
}
