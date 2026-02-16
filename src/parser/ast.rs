//! Abstract Syntax Tree (AST) definitions for Hone
//!
//! The AST represents the parsed structure of a Hone source file.
//! Each node carries source location information for error reporting.

use crate::lexer::token::SourceLocation;

/// A complete Hone file, potentially containing multiple documents
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// Preamble items (before any document separator)
    pub preamble: Vec<PreambleItem>,
    /// Body items (main document content)
    pub body: Vec<BodyItem>,
    /// Additional named documents (after --- separators)
    pub documents: Vec<Document>,
    /// Source location spanning the entire file
    pub location: SourceLocation,
}

/// A named document within a multi-document file
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Document name (from `---name` separator)
    pub name: Option<String>,
    /// Preamble items for this document
    pub preamble: Vec<PreambleItem>,
    /// Body items for this document
    pub body: Vec<BodyItem>,
    /// Source location
    pub location: SourceLocation,
}

/// Items that can appear in the preamble (before body content)
#[derive(Debug, Clone, PartialEq)]
pub enum PreambleItem {
    /// `let name = expr`
    Let(LetBinding),
    /// `from "path" [as alias]`
    From(FromStatement),
    /// `import "path" [as alias]` or `import { a, b } from "path"`
    Import(ImportStatement),
    /// `schema Name { ... }` or `schema Name extends Base { ... }`
    Schema(SchemaDefinition),
    /// `type Name = base_type & constraint1 & constraint2`
    TypeAlias(TypeAliasDefinition),
    /// `use schema_name`
    Use(UseStatement),
    /// `variant name { ... }`
    Variant(VariantDefinition),
    /// `expect args.name: type [= default]`
    Expect(ExpectDeclaration),
    /// `secret name from "provider:path"`
    Secret(SecretDeclaration),
    /// `policy name deny/warn when condition { "message" }`
    Policy(PolicyDeclaration),
}

/// Items that can appear in the body
#[derive(Debug, Clone, PartialEq)]
pub enum BodyItem {
    /// `key: value` or `key +: value` or `key !: value`
    KeyValue(KeyValue),
    /// `name { ... }` - shorthand for `name: { ... }`
    Block(Block),
    /// `when condition { ... }`
    When(WhenBlock),
    /// `for item in iterable { ... }` - only valid inside arrays/objects
    For(ForLoop),
    /// `assert condition [: message]`
    Assert(AssertStatement),
    /// `let name = expr` (also valid in body)
    Let(LetBinding),
    /// Spread: `...expr`
    Spread(SpreadExpr),
}

/// Let binding: `let name = expr`
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
    pub location: SourceLocation,
}

/// From statement: `from "path" [as alias]`
#[derive(Debug, Clone, PartialEq)]
pub struct FromStatement {
    pub path: StringExpr,
    pub alias: Option<String>,
    pub location: SourceLocation,
}

impl FromStatement {
    /// Get the path as a simple string (for non-interpolated paths)
    pub fn parts_as_string(&self) -> String {
        self.path
            .parts
            .iter()
            .filter_map(|p| match p {
                StringPart::Literal(s) => Some(s.as_str()),
                StringPart::Interpolation(_) => None,
            })
            .collect()
    }
}

/// Import statement variants
#[derive(Debug, Clone, PartialEq)]
pub struct ImportStatement {
    pub kind: ImportKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportKind {
    /// `import "path" [as alias]`
    Whole {
        path: StringExpr,
        alias: Option<String>,
    },
    /// `import { a, b as c } from "path"`
    Named {
        names: Vec<ImportName>,
        path: StringExpr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportName {
    pub name: String,
    pub alias: Option<String>,
    pub location: SourceLocation,
}

/// Schema definition
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDefinition {
    pub name: String,
    pub extends: Option<String>,
    pub fields: Vec<SchemaField>,
    /// If true, extra fields beyond the schema are allowed (`...` syntax)
    pub open: bool,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    pub name: String,
    pub constraint: TypeConstraint,
    pub optional: bool,
    pub default: Option<Expr>,
    pub location: SourceLocation,
}

/// Type alias definition: `type Name = base_type & constraint1 & constraint2`
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDefinition {
    pub name: String,
    pub base_type: TypeExpr,
    pub location: SourceLocation,
}

/// Type expression: can be a simple type, union, or intersection with constraints
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// Named type with optional args (e.g., "int", "int(1, 65535)", "Port")
    Named { name: String, args: Vec<Expr> },
    /// Array type (e.g., "array<string>")
    Array(Box<TypeExpr>),
    /// Optional type (e.g., "int?")
    Optional(Box<TypeExpr>),
    /// Union type (e.g., "int | string")
    Union(Vec<TypeExpr>),
}

/// Use statement: `use schema_name`
#[derive(Debug, Clone, PartialEq)]
pub struct UseStatement {
    pub schema_name: String,
    pub location: SourceLocation,
}

/// Variant definition: environment-specific configuration
#[derive(Debug, Clone, PartialEq)]
pub struct VariantDefinition {
    pub name: String,
    pub cases: Vec<VariantCase>,
    pub location: SourceLocation,
}

/// A single case within a variant block
#[derive(Debug, Clone, PartialEq)]
pub struct VariantCase {
    pub name: String,
    pub is_default: bool,
    pub body: Vec<BodyItem>,
    pub location: SourceLocation,
}

/// Expect declaration: `expect args.name: type [= default]`
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectDeclaration {
    /// The full dotted path, e.g. ["args", "env"]
    pub path: Vec<String>,
    /// The expected type name (string, int, bool, float)
    pub type_name: String,
    /// Optional default value
    pub default: Option<Expr>,
    pub location: SourceLocation,
}

/// Policy level: deny (error) or warn (warning)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyLevel {
    Deny,
    Warn,
}

/// Policy declaration: `policy name deny/warn when condition { "message" }`
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyDeclaration {
    /// Policy name (for error reporting)
    pub name: String,
    /// Whether this is a deny (error) or warn (warning)
    pub level: PolicyLevel,
    /// Condition expression (checked against output)
    pub condition: Expr,
    /// Optional message when policy is violated
    pub message: Option<String>,
    pub location: SourceLocation,
}

/// Secret declaration: `secret name from "provider:path"`
#[derive(Debug, Clone, PartialEq)]
pub struct SecretDeclaration {
    /// Variable name for the secret
    pub name: String,
    /// Provider string (e.g., "vault:secret/data/db#password" or "env:API_KEY")
    pub provider: String,
    pub location: SourceLocation,
}

/// Key-value pair with assignment operator
#[derive(Debug, Clone, PartialEq)]
pub struct KeyValue {
    pub key: Key,
    pub op: AssignOp,
    pub value: Expr,
    pub location: SourceLocation,
}

/// Key in a key-value pair
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// Simple identifier: `name`
    Ident(String),
    /// Quoted string: `"name with spaces"`
    String(String),
    /// Computed key: `[expr]`
    Computed(Box<Expr>),
}

/// Assignment operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// `:` - normal assignment (deep merge)
    Colon,
    /// `+:` - array append
    Append,
    /// `!:` - force replace (no merge)
    Replace,
}

/// Block: `name { ... }` - shorthand for key-value with object value
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub items: Vec<BodyItem>,
    pub location: SourceLocation,
}

/// When block: `when condition { ... } [else when condition { ... }] [else { ... }]`
#[derive(Debug, Clone, PartialEq)]
pub struct WhenBlock {
    pub condition: Expr,
    pub body: Vec<BodyItem>,
    pub else_branch: Option<ElseBranch>,
    pub location: SourceLocation,
}

/// Else branch of a when block
#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    /// `else when condition { ... }`
    ElseWhen(Box<WhenBlock>),
    /// `else { ... }`
    Else(Vec<BodyItem>, SourceLocation),
}

/// For loop: `for item in iterable { ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct ForLoop {
    pub binding: ForBinding,
    pub iterable: Expr,
    pub body: ForBody,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForBinding {
    /// Single variable: `for x in ...`
    Single(String),
    /// Destructuring: `for (k, v) in ...`
    Pair(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForBody {
    /// Object body: `{ key: value, ... }`
    Object(Vec<BodyItem>),
    /// Expression body (for arrays): `{ expr }` or just `expr`
    Expr(Expr),
}

/// Assert statement: `assert condition [: message]`
#[derive(Debug, Clone, PartialEq)]
pub struct AssertStatement {
    pub condition: Expr,
    pub message: Option<Expr>,
    pub location: SourceLocation,
}

/// Spread expression: `...expr`
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadExpr {
    pub expr: Expr,
    pub location: SourceLocation,
}

/// Expression node
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Null literal
    Null(SourceLocation),
    /// Boolean literal
    Bool(bool, SourceLocation),
    /// Integer literal
    Integer(i64, SourceLocation),
    /// Float literal
    Float(f64, SourceLocation),
    /// String literal (may contain interpolations)
    String(StringExpr),
    /// Identifier
    Ident(String, SourceLocation),
    /// Path expression: `a.b.c`
    Path(PathExpr),
    /// Array literal: `[a, b, c]`
    Array(ArrayExpr),
    /// Object literal: `{ key: value }`
    Object(ObjectExpr),
    /// Binary operation: `a + b`
    Binary(BinaryExpr),
    /// Unary operation: `!a` or `-a`
    Unary(UnaryExpr),
    /// Function call: `func(args)`
    Call(CallExpr),
    /// Index access: `a[b]`
    Index(IndexExpr),
    /// Conditional: `a ? b : c`
    Conditional(ConditionalExpr),
    /// Type-annotated expression: `value @type(args)`
    Annotated(AnnotatedExpr),
    /// Parenthesized expression: `(expr)`
    Paren(Box<Expr>, SourceLocation),
    /// For expression (in array context)
    For(Box<ForLoop>),
    /// When expression (in array/object context)
    When(Box<WhenBlock>),
}

impl Expr {
    /// Get the source location of this expression
    pub fn location(&self) -> &SourceLocation {
        match self {
            Expr::Null(loc) => loc,
            Expr::Bool(_, loc) => loc,
            Expr::Integer(_, loc) => loc,
            Expr::Float(_, loc) => loc,
            Expr::String(s) => &s.location,
            Expr::Ident(_, loc) => loc,
            Expr::Path(p) => &p.location,
            Expr::Array(a) => &a.location,
            Expr::Object(o) => &o.location,
            Expr::Binary(b) => &b.location,
            Expr::Unary(u) => &u.location,
            Expr::Call(c) => &c.location,
            Expr::Index(i) => &i.location,
            Expr::Conditional(c) => &c.location,
            Expr::Annotated(a) => &a.location,
            Expr::Paren(_, loc) => loc,
            Expr::For(f) => &f.location,
            Expr::When(w) => &w.location,
        }
    }

    /// Human-readable representation of an expression (for error messages)
    pub fn display(&self) -> String {
        match self {
            Expr::Null(_) => "null".to_string(),
            Expr::Bool(b, _) => b.to_string(),
            Expr::Integer(n, _) => n.to_string(),
            Expr::Float(f, _) => f.to_string(),
            Expr::String(s) => {
                let mut result = String::from("\"");
                for part in &s.parts {
                    match part {
                        StringPart::Literal(lit) => result.push_str(lit),
                        StringPart::Interpolation(expr) => {
                            result.push_str("${");
                            result.push_str(&expr.display());
                            result.push('}');
                        }
                    }
                }
                result.push('"');
                result
            }
            Expr::Ident(name, _) => name.clone(),
            Expr::Path(p) => p
                .parts
                .iter()
                .map(|part| match part {
                    PathPart::Ident(name) => name.clone(),
                    PathPart::Index(expr) => format!("[{}]", expr.display()),
                })
                .collect::<Vec<_>>()
                .join("."),
            Expr::Binary(b) => {
                format!("{} {} {}", b.left.display(), b.op, b.right.display())
            }
            Expr::Unary(u) => {
                format!("{}{}", u.op, u.operand.display())
            }
            Expr::Call(c) => {
                let args: Vec<String> = c.args.iter().map(|a| a.display()).collect();
                format!("{}({})", c.func.display(), args.join(", "))
            }
            Expr::Paren(inner, _) => format!("({})", inner.display()),
            Expr::Conditional(c) => {
                format!(
                    "{} ? {} : {}",
                    c.condition.display(),
                    c.then_branch.display(),
                    c.else_branch.display()
                )
            }
            Expr::Index(i) => format!("{}[{}]", i.base.display(), i.index.display()),
            _ => "<expr>".to_string(),
        }
    }

    /// Collect variable names referenced in this expression (for assertion error context)
    pub fn collect_variables(&self) -> Vec<String> {
        let mut vars = Vec::new();
        self.collect_variables_inner(&mut vars);
        vars.dedup();
        vars
    }

    fn collect_variables_inner(&self, vars: &mut Vec<String>) {
        match self {
            Expr::Ident(name, _) => vars.push(name.clone()),
            Expr::Path(p) => {
                let name = p
                    .parts
                    .iter()
                    .map(|part| match part {
                        PathPart::Ident(name) => name.clone(),
                        PathPart::Index(expr) => format!("[{}]", expr.display()),
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                vars.push(name);
            }
            Expr::Binary(b) => {
                b.left.collect_variables_inner(vars);
                b.right.collect_variables_inner(vars);
            }
            Expr::Unary(u) => u.operand.collect_variables_inner(vars),
            Expr::Call(c) => {
                for arg in &c.args {
                    arg.collect_variables_inner(vars);
                }
            }
            Expr::Paren(inner, _) => inner.collect_variables_inner(vars),
            Expr::Conditional(c) => {
                c.condition.collect_variables_inner(vars);
                c.then_branch.collect_variables_inner(vars);
                c.else_branch.collect_variables_inner(vars);
            }
            _ => {}
        }
    }
}

/// String expression, possibly with interpolations
#[derive(Debug, Clone, PartialEq)]
pub struct StringExpr {
    pub parts: Vec<StringPart>,
    pub location: SourceLocation,
}

impl StringExpr {
    /// If this is a plain string literal (no interpolation), return its value.
    pub fn as_literal(&self) -> Option<String> {
        if self.parts.len() == 1 {
            if let StringPart::Literal(s) = &self.parts[0] {
                return Some(s.clone());
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    /// Literal string content
    Literal(String),
    /// Interpolated expression: `${expr}`
    Interpolation(Expr),
}

/// Path expression: `a.b.c`
#[derive(Debug, Clone, PartialEq)]
pub struct PathExpr {
    pub parts: Vec<PathPart>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathPart {
    /// Identifier: `.name`
    Ident(String),
    /// Index: `[expr]`
    Index(Expr),
}

/// Array literal
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayExpr {
    pub elements: Vec<ArrayElement>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrayElement {
    /// Simple expression
    Expr(Expr),
    /// Spread: `...expr`
    Spread(Expr),
    /// For comprehension
    For(ForLoop),
    /// Conditional element
    When(WhenBlock),
}

/// Object literal
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectExpr {
    pub items: Vec<BodyItem>,
    pub location: SourceLocation,
}

/// Binary expression
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Logical
    And,
    Or,
    // Null coalescing
    NullCoalesce,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::NotEq => write!(f, "!="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::LtEq => write!(f, "<="),
            BinaryOp::GtEq => write!(f, ">="),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
            BinaryOp::NullCoalesce => write!(f, "??"),
        }
    }
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::Neg => write!(f, "-"),
        }
    }
}

impl BinaryOp {
    /// Get the precedence of this operator (higher = tighter binding)
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::NotEq => 3,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::LtEq | BinaryOp::GtEq => 4,
            BinaryOp::NullCoalesce => 5,
            BinaryOp::Add | BinaryOp::Sub => 6,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 7,
        }
    }
}

/// Unary expression
#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub operand: Box<Expr>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

/// Function call
#[derive(Debug, Clone, PartialEq)]
pub struct CallExpr {
    pub func: Box<Expr>,
    pub args: Vec<Expr>,
    pub location: SourceLocation,
}

/// Index access
#[derive(Debug, Clone, PartialEq)]
pub struct IndexExpr {
    pub base: Box<Expr>,
    pub index: Box<Expr>,
    pub location: SourceLocation,
}

/// Conditional expression: `a ? b : c`
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalExpr {
    pub condition: Box<Expr>,
    pub then_branch: Box<Expr>,
    pub else_branch: Box<Expr>,
    pub location: SourceLocation,
}

/// Type-annotated expression: `value @type(args)`
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedExpr {
    pub expr: Box<Expr>,
    pub constraint: TypeConstraint,
    pub location: SourceLocation,
}

/// Type constraint annotation
#[derive(Debug, Clone, PartialEq)]
pub struct TypeConstraint {
    pub name: String,
    pub args: Vec<Expr>,
    pub location: SourceLocation,
}
