//! Error types, diagnostics, and result aliases for the Hone compiler.
//!
//! All user-facing errors are variants of [`HoneError`], rendered via `miette` diagnostics.

use std::path::PathBuf;

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::lexer::token::SourceLocation;

/// Warning from compilation (non-fatal)
#[derive(Debug, Clone)]
pub struct Warning {
    pub message: String,
    pub file: Option<PathBuf>,
    pub line: usize,
    pub column: usize,
}

/// Calculate Levenshtein distance between two strings
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(a_len + 1) {
        row[0] = i;
    }
    for (j, val) in matrix[0].iter_mut().enumerate().take(b_len + 1) {
        *val = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Find the best "did you mean?" suggestion from a list of candidates
pub fn find_similar(name: &str, candidates: &[String], max_distance: usize) -> Option<String> {
    let name_lower = name.to_lowercase();
    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = levenshtein_distance(&name_lower, &candidate_lower);

        // Only suggest if distance is within threshold and better than current best
        if distance <= max_distance && distance < best_distance {
            best_distance = distance;
            best_match = Some(candidate.clone());
        }
    }

    best_match
}

/// Generate a help message for an undefined variable with suggestions
pub fn undefined_variable_help(name: &str, available: &[String]) -> String {
    // Calculate max distance based on name length (longer names allow more typos)
    let max_distance = (name.len() / 3).clamp(2, 3);

    if let Some(suggestion) = find_similar(name, available, max_distance) {
        format!("did you mean '{}'?", suggestion)
    } else if available.is_empty() {
        "no variables are defined in this scope".to_string()
    } else if available.len() <= 5 {
        format!("available variables: {}", available.join(", "))
    } else {
        "check variable name for typos".to_string()
    }
}

/// Error codes following the DESIGN.md specification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Syntax Errors (E0xxx)
    E0001, // Unexpected token
    E0002, // Undefined variable
    E0003, // Reserved word as bare key
    E0004, // Unterminated string
    E0005, // Invalid escape sequence

    // Import Errors (E01xx)
    E0101, // File not found
    E0102, // Circular import
    E0103, // Import resolution failed (interpolation in path)

    // Type Errors (E02xx)
    E0201, // Value out of range
    E0202, // Type mismatch
    E0203, // Pattern mismatch
    E0204, // Required field missing
    E0205, // Unknown field in closed schema
    E0206, // Required field set to null

    // Merge Errors (E03xx)
    E0301, // Type conflict during merge
    E0302, // Multiple from declarations
    E0303, // Append to non-array
    E0304, // from in preamble of multi-document
    E0305, // No matching document in base
    E0306, // Cannot inherit from multi-document base

    // Evaluation Errors (E04xx)
    E0401, // Missing required argument
    E0402, // Division by zero
    E0403, // Array index out of bounds

    // Dependency Errors (E05xx)
    E0501, // Circular dependency

    // Function Errors (E06xx)
    E0601, // Function type error
    E0602, // Wrong number of arguments
    E0603, // Undefined function

    // Control Flow Errors (E07xx)
    E0701, // for not allowed at top level
    E0702, // Assertion failed

    // Hermeticity Errors (E08xx)
    E0801, // env/file requires --allow-env
    E0802, // secret placeholder in output
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::E0001 => write!(f, "E0001"),
            ErrorCode::E0002 => write!(f, "E0002"),
            ErrorCode::E0003 => write!(f, "E0003"),
            ErrorCode::E0004 => write!(f, "E0004"),
            ErrorCode::E0005 => write!(f, "E0005"),
            ErrorCode::E0101 => write!(f, "E0101"),
            ErrorCode::E0102 => write!(f, "E0102"),
            ErrorCode::E0103 => write!(f, "E0103"),
            ErrorCode::E0201 => write!(f, "E0201"),
            ErrorCode::E0202 => write!(f, "E0202"),
            ErrorCode::E0203 => write!(f, "E0203"),
            ErrorCode::E0204 => write!(f, "E0204"),
            ErrorCode::E0205 => write!(f, "E0205"),
            ErrorCode::E0206 => write!(f, "E0206"),
            ErrorCode::E0301 => write!(f, "E0301"),
            ErrorCode::E0302 => write!(f, "E0302"),
            ErrorCode::E0303 => write!(f, "E0303"),
            ErrorCode::E0304 => write!(f, "E0304"),
            ErrorCode::E0305 => write!(f, "E0305"),
            ErrorCode::E0306 => write!(f, "E0306"),
            ErrorCode::E0401 => write!(f, "E0401"),
            ErrorCode::E0402 => write!(f, "E0402"),
            ErrorCode::E0403 => write!(f, "E0403"),
            ErrorCode::E0501 => write!(f, "E0501"),
            ErrorCode::E0601 => write!(f, "E0601"),
            ErrorCode::E0602 => write!(f, "E0602"),
            ErrorCode::E0603 => write!(f, "E0603"),
            ErrorCode::E0701 => write!(f, "E0701"),
            ErrorCode::E0702 => write!(f, "E0702"),
            ErrorCode::E0801 => write!(f, "E0801"),
            ErrorCode::E0802 => write!(f, "E0802"),
        }
    }
}

/// Main error type for Hone compiler
#[derive(Error, Debug, Diagnostic)]
pub enum HoneError {
    #[error("unexpected token")]
    #[diagnostic(code(E0001), help("{help}"))]
    UnexpectedToken {
        #[source_code]
        src: String,
        #[label("unexpected: {found}")]
        span: SourceSpan,
        expected: String,
        found: String,
        help: String,
    },

    #[error("undefined variable")]
    #[diagnostic(code(E0002), help("{help}"))]
    UndefinedVariable {
        #[source_code]
        src: String,
        #[label("'{name}' is not defined in this scope")]
        span: SourceSpan,
        name: String,
        help: String,
    },

    #[error("reserved word cannot be used as bare key")]
    #[diagnostic(code(E0003), help("quote the key: \"{keyword}\": ..."))]
    ReservedWordAsKey {
        #[source_code]
        src: String,
        #[label("'{keyword}' is a reserved word")]
        span: SourceSpan,
        keyword: String,
    },

    #[error("unterminated string")]
    #[diagnostic(code(E0004), help("add closing quote at end of string"))]
    UnterminatedString {
        #[source_code]
        src: String,
        #[label("string started here but never closed")]
        span: SourceSpan,
    },

    #[error("invalid escape sequence")]
    #[diagnostic(code(E0005), help("{help}"))]
    InvalidEscapeSequence {
        #[source_code]
        src: String,
        #[label("invalid escape: {sequence}")]
        span: SourceSpan,
        sequence: String,
        help: String,
    },

    #[error("unexpected character")]
    #[diagnostic(code(E0001), help("{help}"))]
    UnexpectedCharacter {
        #[source_code]
        src: String,
        #[label("unexpected: '{ch}'")]
        span: SourceSpan,
        ch: char,
        help: String,
    },

    #[error("import not found")]
    #[diagnostic(code(E0101))]
    ImportNotFound {
        #[source_code]
        src: String,
        #[label("file not found: {path}")]
        span: SourceSpan,
        path: String,
    },

    #[error("circular import detected")]
    #[diagnostic(code(E0102), help("{chain}"))]
    CircularImport {
        #[source_code]
        src: String,
        #[label("cycle detected here")]
        span: SourceSpan,
        chain: String,
    },

    #[error("value out of range")]
    #[diagnostic(code(E0201), help("{help}"))]
    ValueOutOfRange {
        #[source_code]
        src: String,
        #[label("value: {value}")]
        span: SourceSpan,
        expected: String,
        value: String,
        help: String,
    },

    #[error("type mismatch")]
    #[diagnostic(code(E0202), help("{help}"))]
    TypeMismatch {
        #[source_code]
        src: String,
        #[label("expected: {expected}, found: {found}")]
        span: SourceSpan,
        expected: String,
        found: String,
        help: String,
    },

    #[error("missing required field")]
    #[diagnostic(
        code(E0204),
        help("add the missing field '{field}' to satisfy schema '{schema}'")
    )]
    MissingField {
        #[source_code]
        src: String,
        #[label("missing field: '{field}'")]
        span: SourceSpan,
        field: String,
        schema: String,
    },

    #[error("unknown field in closed schema")]
    #[diagnostic(code(E0205), help("{help}"))]
    UnknownField {
        #[source_code]
        src: String,
        #[label("'{field}' is not defined in schema '{schema}'")]
        span: SourceSpan,
        field: String,
        schema: String,
        help: String,
    },

    #[error("pattern mismatch")]
    #[diagnostic(code(E0203), help("{help}"))]
    PatternMismatch {
        #[source_code]
        src: String,
        #[label("does not match pattern")]
        span: SourceSpan,
        pattern: String,
        value: String,
        help: String,
    },

    #[error("multiple 'from' declarations")]
    #[diagnostic(code(E0302), help("a file may only inherit from one base"))]
    MultipleFrom {
        #[source_code]
        src: String,
        #[label("duplicate 'from'")]
        span: SourceSpan,
        #[label("first 'from' here")]
        first_span: SourceSpan,
    },

    #[error("'from' not allowed in preamble of multi-document file")]
    #[diagnostic(
        code(E0304),
        help("in multi-document files, each document must declare its own 'from'")
    )]
    FromInPreamble {
        #[source_code]
        src: String,
        #[label("'from' cannot be in preamble")]
        span: SourceSpan,
    },

    #[error("circular dependency")]
    #[diagnostic(code(E0501), help("{help}"))]
    CircularDependency {
        #[source_code]
        src: String,
        #[label("cycle: {cycle}")]
        span: SourceSpan,
        cycle: String,
        help: String,
    },

    #[error("'for' not allowed at top level")]
    #[diagnostic(
        code(E0701),
        help("'for' blocks are only valid inside arrays or objects")
    )]
    ForAtTopLevel {
        #[source_code]
        src: String,
        #[label("'for' not allowed here")]
        span: SourceSpan,
    },

    #[error("assertion failed: {message}")]
    #[diagnostic(code(E0702), help("{help}"))]
    AssertionFailed {
        #[source_code]
        src: String,
        #[label("condition: {condition}")]
        span: SourceSpan,
        condition: String,
        message: String,
        help: String,
    },

    #[error("arithmetic overflow")]
    #[diagnostic(code(E0402), help("{help}"))]
    ArithmeticOverflow {
        #[source_code]
        src: String,
        #[label("{operation}")]
        span: SourceSpan,
        operation: String,
        help: String,
    },

    #[error("division by zero")]
    #[diagnostic(code(E0402), help("divisor must be non-zero"))]
    DivisionByZero {
        #[source_code]
        src: String,
        #[label("division by zero here")]
        span: SourceSpan,
    },

    #[error("{func_name}() requires --allow-env flag")]
    #[diagnostic(code(E0801), help("{help}"))]
    EnvNotAllowed {
        #[source_code]
        src: String,
        #[label("{func_name}() reads external state")]
        span: SourceSpan,
        func_name: String,
        help: String,
    },

    #[error("maximum nesting depth exceeded")]
    #[diagnostic(code(E0403), help("{help}"))]
    RecursionLimitExceeded {
        #[source_code]
        src: String,
        #[label("nesting too deep here")]
        span: SourceSpan,
        help: String,
    },

    #[error("secret placeholder in output")]
    #[diagnostic(code(E0802), help("{help}"))]
    SecretInOutput {
        #[source_code]
        src: String,
        #[label("secret value at path: {path}")]
        span: SourceSpan,
        path: String,
        help: String,
    },

    #[error("schema validation failed ({count} error{s})")]
    #[diagnostic(help("fix all schema violations listed below"))]
    SchemaValidationErrors {
        #[source_code]
        src: String,
        #[label("schema applied here")]
        span: SourceSpan,
        count: usize,
        s: String,
        #[related]
        errors: Vec<HoneError>,
    },

    #[error("I/O error: {message}")]
    IoError { message: String },

    #[error("{message}")]
    CompilationError { message: String },
}

impl HoneError {
    /// Create an UnexpectedToken error
    pub fn unexpected_token(
        src: impl Into<String>,
        location: &SourceLocation,
        expected: impl Into<String>,
        found: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        HoneError::UnexpectedToken {
            src: src.into(),
            span: (location.offset, location.length).into(),
            expected: expected.into(),
            found: found.into(),
            help: help.into(),
        }
    }

    /// Create an UndefinedVariable error
    pub fn undefined_variable(
        src: impl Into<String>,
        location: &SourceLocation,
        name: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        HoneError::UndefinedVariable {
            src: src.into(),
            span: (location.offset, location.length).into(),
            name: name.into(),
            help: help.into(),
        }
    }

    /// Create an UnterminatedString error
    pub fn unterminated_string(src: impl Into<String>, location: &SourceLocation) -> Self {
        HoneError::UnterminatedString {
            src: src.into(),
            span: (location.offset, location.length).into(),
        }
    }

    /// Create an InvalidEscapeSequence error
    pub fn invalid_escape_sequence(
        src: impl Into<String>,
        location: &SourceLocation,
        sequence: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        HoneError::InvalidEscapeSequence {
            src: src.into(),
            span: (location.offset, location.length).into(),
            sequence: sequence.into(),
            help: help.into(),
        }
    }

    /// Create an UnexpectedCharacter error
    pub fn unexpected_character(
        src: impl Into<String>,
        location: &SourceLocation,
        ch: char,
    ) -> Self {
        let help = if ch == '\t' {
            "Hone uses spaces for indentation, not tabs".to_string()
        } else if ch == ';' {
            "Hone does not use semicolons — use newlines to separate statements".to_string()
        } else if ch == '`' {
            "use double quotes \"...\" or single quotes '...' for strings".to_string()
        } else {
            format!("'{}' is not valid Hone syntax", ch)
        };
        HoneError::UnexpectedCharacter {
            src: src.into(),
            span: (location.offset, location.length).into(),
            ch,
            help,
        }
    }

    /// Create an IoError
    pub fn io_error(message: impl Into<String>) -> Self {
        HoneError::IoError {
            message: message.into(),
        }
    }

    /// Create a CompilationError (for CLI-level compilation failures like --strict)
    pub fn compilation_error(message: impl Into<String>) -> Self {
        HoneError::CompilationError {
            message: message.into(),
        }
    }

    /// Get the span (start, end) for this error, if it has one
    pub fn span(&self) -> Option<Span> {
        match self {
            HoneError::UnexpectedToken { span, .. } => Some(Span::from(*span)),
            HoneError::UndefinedVariable { span, .. } => Some(Span::from(*span)),
            HoneError::ReservedWordAsKey { span, .. } => Some(Span::from(*span)),
            HoneError::UnterminatedString { span, .. } => Some(Span::from(*span)),
            HoneError::InvalidEscapeSequence { span, .. } => Some(Span::from(*span)),
            HoneError::UnexpectedCharacter { span, .. } => Some(Span::from(*span)),
            HoneError::ImportNotFound { span, .. } => Some(Span::from(*span)),
            HoneError::CircularImport { span, .. } => Some(Span::from(*span)),
            HoneError::ValueOutOfRange { span, .. } => Some(Span::from(*span)),
            HoneError::TypeMismatch { span, .. } => Some(Span::from(*span)),
            HoneError::MissingField { span, .. } => Some(Span::from(*span)),
            HoneError::UnknownField { span, .. } => Some(Span::from(*span)),
            HoneError::PatternMismatch { span, .. } => Some(Span::from(*span)),
            HoneError::MultipleFrom { span, .. } => Some(Span::from(*span)),
            HoneError::FromInPreamble { span, .. } => Some(Span::from(*span)),
            HoneError::CircularDependency { span, .. } => Some(Span::from(*span)),
            HoneError::ForAtTopLevel { span, .. } => Some(Span::from(*span)),
            HoneError::AssertionFailed { span, .. } => Some(Span::from(*span)),
            HoneError::ArithmeticOverflow { span, .. } => Some(Span::from(*span)),
            HoneError::DivisionByZero { span, .. } => Some(Span::from(*span)),
            HoneError::EnvNotAllowed { span, .. } => Some(Span::from(*span)),
            HoneError::RecursionLimitExceeded { span, .. } => Some(Span::from(*span)),
            HoneError::SecretInOutput { span, .. } => Some(Span::from(*span)),
            HoneError::SchemaValidationErrors { span, .. } => Some(Span::from(*span)),
            HoneError::IoError { .. } => None,
            HoneError::CompilationError { .. } => None,
        }
    }

    /// Get a simple error message (without source context)
    pub fn message(&self) -> String {
        match self {
            HoneError::UnexpectedToken {
                expected, found, ..
            } => {
                format!("unexpected token: expected {}, found {}", expected, found)
            }
            HoneError::UndefinedVariable { name, .. } => {
                format!("undefined variable: '{}'", name)
            }
            HoneError::ReservedWordAsKey { keyword, .. } => {
                format!("reserved word '{}' cannot be used as bare key", keyword)
            }
            HoneError::UnterminatedString { .. } => "unterminated string".to_string(),
            HoneError::InvalidEscapeSequence { sequence, .. } => {
                format!("invalid escape sequence: {}", sequence)
            }
            HoneError::UnexpectedCharacter { ch, .. } => {
                format!("unexpected character: '{}'", ch)
            }
            HoneError::ImportNotFound { path, .. } => {
                format!("import not found: {}", path)
            }
            HoneError::CircularImport { chain, .. } => {
                format!("circular import detected: {}", chain)
            }
            HoneError::ValueOutOfRange {
                expected, value, ..
            } => {
                format!("value out of range: expected {}, got {}", expected, value)
            }
            HoneError::TypeMismatch {
                expected, found, ..
            } => {
                format!("type mismatch: expected {}, found {}", expected, found)
            }
            HoneError::MissingField { field, schema, .. } => {
                format!("missing required field '{}' for schema '{}'", field, schema)
            }
            HoneError::UnknownField { field, schema, .. } => {
                format!("unknown field '{}' in closed schema '{}'", field, schema)
            }
            HoneError::PatternMismatch { pattern, value, .. } => {
                format!("value \"{}\" does not match pattern /{}/", value, pattern)
            }
            HoneError::MultipleFrom { .. } => "multiple 'from' declarations".to_string(),
            HoneError::FromInPreamble { .. } => {
                "'from' not allowed in preamble of multi-document file".to_string()
            }
            HoneError::CircularDependency { cycle, .. } => {
                format!("circular dependency: {}", cycle)
            }
            HoneError::ForAtTopLevel { .. } => "'for' not allowed at top level".to_string(),
            HoneError::AssertionFailed { message, .. } => {
                format!("assertion failed: {}", message)
            }
            HoneError::ArithmeticOverflow { operation, .. } => {
                format!("arithmetic overflow: {}", operation)
            }
            HoneError::DivisionByZero { .. } => "division by zero".to_string(),
            HoneError::EnvNotAllowed { func_name, .. } => {
                format!("{}() requires --allow-env flag", func_name)
            }
            HoneError::RecursionLimitExceeded { .. } => {
                "maximum nesting depth exceeded".to_string()
            }
            HoneError::SecretInOutput { path, .. } => {
                format!("secret placeholder in output at path: {}", path)
            }
            HoneError::SchemaValidationErrors { count, errors, .. } => {
                let msgs: Vec<String> = errors.iter().map(|e| e.message()).collect();
                format!(
                    "schema validation failed ({} error{}): {}",
                    count,
                    if *count == 1 { "" } else { "s" },
                    msgs.join("; ")
                )
            }
            HoneError::IoError { message } => format!("I/O error: {}", message),
            HoneError::CompilationError { message } => message.clone(),
        }
    }
}

/// Simple span type for LSP (offset, length) -> (start, end)
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl From<SourceSpan> for Span {
    fn from(span: SourceSpan) -> Self {
        Self {
            start: span.offset(),
            end: span.offset() + span.len(),
        }
    }
}

/// Result type for Hone operations
pub type HoneResult<T> = Result<T, HoneError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("abc", "abcd"), 1);
        assert_eq!(levenshtein_distance("port", "prot"), 2);
        assert_eq!(levenshtein_distance("config", "confgi"), 2);
    }

    #[test]
    fn test_find_similar_exact_match() {
        let candidates = vec!["port".to_string(), "host".to_string(), "name".to_string()];
        assert_eq!(
            find_similar("port", &candidates, 2),
            Some("port".to_string())
        );
    }

    #[test]
    fn test_find_similar_typo() {
        let candidates = vec!["port".to_string(), "host".to_string(), "name".to_string()];
        assert_eq!(
            find_similar("prot", &candidates, 2),
            Some("port".to_string())
        );
        assert_eq!(
            find_similar("hsot", &candidates, 2),
            Some("host".to_string())
        );
    }

    #[test]
    fn test_find_similar_no_match() {
        let candidates = vec!["port".to_string(), "host".to_string(), "name".to_string()];
        assert_eq!(find_similar("xyz", &candidates, 2), None);
    }

    #[test]
    fn test_find_similar_case_insensitive() {
        let candidates = vec!["Port".to_string(), "HOST".to_string()];
        assert_eq!(
            find_similar("port", &candidates, 2),
            Some("Port".to_string())
        );
        assert_eq!(
            find_similar("host", &candidates, 2),
            Some("HOST".to_string())
        );
    }

    #[test]
    fn test_undefined_variable_help_with_suggestion() {
        let available = vec!["port".to_string(), "host".to_string(), "name".to_string()];
        let help = undefined_variable_help("prot", &available);
        assert!(help.contains("did you mean"));
        assert!(help.contains("port"));
    }

    #[test]
    fn test_undefined_variable_help_no_match() {
        let available = vec!["port".to_string(), "host".to_string()];
        let help = undefined_variable_help("xyz", &available);
        assert!(help.contains("available variables") || help.contains("check variable name"));
    }

    #[test]
    fn test_undefined_variable_help_empty() {
        let available: Vec<String> = vec![];
        let help = undefined_variable_help("xyz", &available);
        assert!(help.contains("no variables are defined"));
    }
}

/// Collection of errors (for error recovery)
#[derive(Debug, Default)]
pub struct ErrorCollection {
    errors: Vec<HoneError>,
    max_errors: usize,
}

impl ErrorCollection {
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: Vec::new(),
            max_errors,
        }
    }

    pub fn push(&mut self, error: HoneError) -> bool {
        self.errors.push(error);
        self.errors.len() >= self.max_errors
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    pub fn errors(&self) -> &[HoneError] {
        &self.errors
    }

    pub fn into_errors(self) -> Vec<HoneError> {
        self.errors
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}
