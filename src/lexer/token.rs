use std::fmt;
use std::path::PathBuf;

/// Source location information for error reporting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// File path (if known)
    pub file: Option<PathBuf>,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Byte offset from start of file
    pub offset: usize,
    /// Length in bytes
    pub length: usize,
}

impl SourceLocation {
    pub fn new(
        file: Option<PathBuf>,
        line: usize,
        column: usize,
        offset: usize,
        length: usize,
    ) -> Self {
        Self {
            file,
            line,
            column,
            offset,
            length,
        }
    }

    /// Create a span from this location to another
    pub fn span_to(&self, other: &SourceLocation) -> SourceLocation {
        SourceLocation {
            file: self.file.clone(),
            line: self.line,
            column: self.column,
            offset: self.offset,
            length: (other.offset + other.length).saturating_sub(self.offset),
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(path) => write!(f, "{}:{}:{}", path.display(), self.line, self.column),
            None => write!(f, "<input>:{}:{}", self.line, self.column),
        }
    }
}

/// Token type enumeration - all possible tokens in Hone
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Let,
    From,
    Import,
    As,
    When,
    For,
    In,
    Schema,
    Type,
    Assert,
    Use,
    Extends,
    Doc,
    Variant,
    Else,
    Expect,
    Secret,
    Policy,
    Deny,
    Warn,
    Fn,
    Null,
    True,
    False,

    // Literals
    Integer(i64),
    Float(f64),
    String(String),

    // String interpolation parts (for "text ${expr} more text")
    StringStart(String),  // "text ${
    StringMiddle(String), // } middle ${
    StringEnd(String),    // } end"

    // Triple-quoted strings
    TripleString(String),

    // Identifiers
    Ident(String),

    // Punctuation
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]
    LeftParen,    // (
    RightParen,   // )
    Colon,        // :
    ColonPlus,    // +:
    ColonBang,    // !:
    At,           // @
    Dot,          // .
    Comma,        // ,
    DocSeparator, // ---

    // Operators
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    EqEq,      // ==
    NotEq,     // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    And,       // &&
    Ampersand, // &
    Or,        // ||
    Not,       // !
    Question,  // ?
    Eq,        // =
    Pipe,      // |

    // Special
    Newline,
    Eof,
}

impl TokenKind {
    /// Check if this token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Let
                | TokenKind::From
                | TokenKind::Import
                | TokenKind::As
                | TokenKind::When
                | TokenKind::For
                | TokenKind::In
                | TokenKind::Schema
                | TokenKind::Type
                | TokenKind::Assert
                | TokenKind::Use
                | TokenKind::Extends
                | TokenKind::Doc
                | TokenKind::Variant
                | TokenKind::Else
                | TokenKind::Expect
                | TokenKind::Secret
                | TokenKind::Policy
                | TokenKind::Deny
                | TokenKind::Warn
                | TokenKind::Fn
                | TokenKind::Null
                | TokenKind::True
                | TokenKind::False
        )
    }

    /// Get the keyword from a string, if it matches
    pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
        match s {
            "let" => Some(TokenKind::Let),
            "from" => Some(TokenKind::From),
            "import" => Some(TokenKind::Import),
            "as" => Some(TokenKind::As),
            "when" => Some(TokenKind::When),
            "for" => Some(TokenKind::For),
            "in" => Some(TokenKind::In),
            "schema" => Some(TokenKind::Schema),
            "type" => Some(TokenKind::Type),
            "assert" => Some(TokenKind::Assert),
            "use" => Some(TokenKind::Use),
            "extends" => Some(TokenKind::Extends),
            "doc" => Some(TokenKind::Doc),
            "variant" => Some(TokenKind::Variant),
            "else" => Some(TokenKind::Else),
            "expect" => Some(TokenKind::Expect),
            "secret" => Some(TokenKind::Secret),
            "policy" => Some(TokenKind::Policy),
            "deny" => Some(TokenKind::Deny),
            "warn" => Some(TokenKind::Warn),
            "fn" => Some(TokenKind::Fn),
            "null" => Some(TokenKind::Null),
            "true" => Some(TokenKind::True),
            "false" => Some(TokenKind::False),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Let => write!(f, "let"),
            TokenKind::From => write!(f, "from"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::As => write!(f, "as"),
            TokenKind::When => write!(f, "when"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Schema => write!(f, "schema"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::Assert => write!(f, "assert"),
            TokenKind::Use => write!(f, "use"),
            TokenKind::Extends => write!(f, "extends"),
            TokenKind::Doc => write!(f, "doc"),
            TokenKind::Variant => write!(f, "variant"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Expect => write!(f, "expect"),
            TokenKind::Secret => write!(f, "secret"),
            TokenKind::Policy => write!(f, "policy"),
            TokenKind::Deny => write!(f, "deny"),
            TokenKind::Warn => write!(f, "warn"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Null => write!(f, "null"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Integer(n) => write!(f, "{}", n),
            TokenKind::Float(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::StringStart(s) => write!(f, "\"{}${{", s),
            TokenKind::StringMiddle(s) => write!(f, "}}{}${{", s),
            TokenKind::StringEnd(s) => write!(f, "}}{}\"", s),
            TokenKind::TripleString(s) => write!(f, "\"\"\"{}\"\"\"", s),
            TokenKind::Ident(s) => write!(f, "{}", s),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::ColonPlus => write!(f, "+:"),
            TokenKind::ColonBang => write!(f, "!:"),
            TokenKind::At => write!(f, "@"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Comma => write!(f, ","),
            TokenKind::DocSeparator => write!(f, "---"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::NotEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::And => write!(f, "&&"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::Not => write!(f, "!"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Eq => write!(f, "="),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Newline => write!(f, "<newline>"),
            TokenKind::Eof => write!(f, "<eof>"),
        }
    }
}

/// A token with its location and kind
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub location: SourceLocation,
}

impl Token {
    pub fn new(kind: TokenKind, location: SourceLocation) -> Self {
        Self { kind, location }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.kind, self.location)
    }
}
