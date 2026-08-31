use indexmap::IndexMap;
use mcl_spec::Span;

/// Top-level MCL file — the parser determines which variant from the first token
#[derive(Debug, Clone)]
pub enum MclFile {
    Graph(GraphFile),
    Context(Box<ContextFile>),
    Domain(DomainFile),
    Template(TemplateFile),
}

// ─── Graph files ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GraphFile {
    pub name: String,
    pub domain: Option<String>,
    pub nodes: Vec<NodeDecl>,
    pub channels: Vec<ChannelDecl>,
    pub edges: Vec<EdgeDecl>,
    pub memories: Vec<MemoryDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NodeDecl {
    pub name: String,
    pub node_type: String,
    pub config: IndexMap<String, ValueExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ChannelDecl {
    pub name: String,
    pub payload_schema: Option<IndexMap<String, TypeExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EdgeDecl {
    pub from: String,
    pub to: String,
    pub channel: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MemoryDecl {
    pub node: String,
    pub config: IndexMap<String, ValueExpr>,
    pub span: Span,
}

// ─── Context files ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ContextFile {
    pub frontmatter: Frontmatter,
    pub state: Vec<StateVar>,
    pub entities: Vec<EntityDecl>,
    pub items: Vec<ItemDecl>,
    pub actions: Vec<ActionDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub narrator: Option<NarratorDecl>,
    pub knowledge: Vec<KnowledgeEntry>,
    pub constraints: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub entries: IndexMap<String, String>,
    pub span: Span,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct StateVar {
    pub name: String,
    pub type_constraint: Option<TypeExpr>,
    pub default: Option<ValueExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub name: String,
    pub entity_type: Option<String>,
    pub extends: Option<String>,
    pub properties: IndexMap<String, ValueExpr>,
    pub knows: Vec<String>,
    /// Inline constraint tags for entity dialogue/behavior
    pub constraint_tags: Vec<crate::constraints::ConstraintTag>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ItemDecl {
    pub name: String,
    pub properties: IndexMap<String, ValueExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActionDecl {
    pub name: String,
    pub requires: Vec<ConditionExpr>,
    pub mutates: IndexMap<String, ValueExpr>,
    pub resolution: Option<String>,
    pub target: Option<TargetExpr>,
    pub mode: Option<String>,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TransitionDecl {
    pub target: String,
    pub when: Option<ConditionExpr>,
    pub direction: Option<String>,
    pub auto_: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NarratorDecl {
    pub tone: Option<String>,
    pub voice: Option<String>,
    pub tense: Option<String>,
    pub forbidden: Vec<String>,
    pub context: Option<String>,
    pub style: Option<String>,
    /// Inline constraint tags parsed from prose blocks
    pub constraint_tags: Vec<crate::constraints::ConstraintTag>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum KnowledgeEntry {
    Knows(String),
    Ignores(String),
}

// ─── Domain files ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DomainFile {
    pub name: String,
    pub resolvers: Vec<String>,
    pub types: Vec<TypeDecl>,
    pub schemas: Vec<SchemaDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub fields: IndexMap<String, TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchemaDecl {
    pub name: String,
    pub fields: Vec<StateVar>,
    pub span: Span,
}

// ─── Template files ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub templates: Vec<TemplateDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TemplateDecl {
    pub name: String,
    pub extends: Option<String>,
    pub properties: IndexMap<String, ValueExpr>,
    pub span: Span,
}

// ─── Expressions ────────────────────────────────────────────────────────

/// Value expression — a literal or reference
#[derive(Debug, Clone)]
pub enum ValueExpr {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Ident(String), // bare identifier (e.g., `locked`, `dread`)
    List(Vec<ValueExpr>),
    Map(IndexMap<String, ValueExpr>),
}

/// Type expression — CUE-inspired constraint types
#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// Named primitive: bool, int, float, string, any, signal
    Named(String),
    /// List type: [T]
    ListOf(Box<TypeExpr>),
    /// Struct type: { key: T, ... }
    Struct(IndexMap<String, TypeExpr>),
    /// Enum/union: "a" | "b" | "c"
    Union(Vec<TypeExpr>),
    /// String literal as type: "locked"
    StringLiteral(String),
    /// Range constraint: >=0, <=100
    RangeGe(i64),
    RangeLe(i64),
    /// Intersection: T & constraint
    Intersection(Vec<TypeExpr>),
    /// Optional: T?
    Optional(Box<TypeExpr>),
}

/// Condition expression for `when:`, `requires:`, etc.
#[derive(Debug, Clone)]
pub enum ConditionExpr {
    Compare {
        left: String, // field path
        op: CmpOp,
        right: ValueExpr,
    },
    Truthy(String), // field path is truthy
    Falsy(String),  // !field path
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

/// Target expression for actions
#[derive(Debug, Clone)]
pub enum TargetExpr {
    Fixed(String),
    Enum {
        source: String,         // e.g., "entities"
        filter: Option<String>, // e.g., "type == enemy"
    },
}
