//! Lexer (tokenizer) for the Hone configuration language.
//!
//! Converts source text into a stream of [`token::Token`]s for the parser.

pub mod token;

use std::path::PathBuf;

use crate::errors::{HoneError, HoneResult};
use token::{SourceLocation, Token, TokenKind};

/// A collected comment with its location
#[derive(Debug, Clone)]
pub struct Comment {
    /// The comment text (without the leading # or /* */)
    pub text: String,
    /// Line number where the comment starts (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Whether this is a block comment (/* ... */)
    pub is_block: bool,
}

/// Lexer for Hone configuration language
pub struct Lexer<'a> {
    /// Source code being lexed
    source: &'a str,
    /// Characters as bytes for iteration
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    /// Current position in bytes
    position: usize,
    /// Current line number (1-indexed)
    line: usize,
    /// Current column number (1-indexed)
    column: usize,
    /// Start position of current token
    token_start: usize,
    /// Start line of current token
    token_start_line: usize,
    /// Start column of current token
    token_start_column: usize,
    /// File path for error reporting
    file: Option<PathBuf>,
    /// Track if we're inside string interpolation
    interpolation_depth: usize,
    /// Stack for tracking brace depth in interpolations
    interpolation_brace_stack: Vec<usize>,
    /// Track whether each interpolation level is inside a triple-quoted string
    triple_string_interpolation: Vec<bool>,
    /// Collected comments (for formatter use)
    comments: Vec<Comment>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'a str, file: Option<PathBuf>) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            position: 0,
            line: 1,
            column: 1,
            token_start: 0,
            token_start_line: 1,
            token_start_column: 1,
            file,
            interpolation_depth: 0,
            interpolation_brace_stack: Vec::new(),
            triple_string_interpolation: Vec::new(),
            comments: Vec::new(),
        }
    }

    /// Get the source code
    pub fn source(&self) -> &str {
        self.source
    }

    /// Get collected comments (available after tokenization)
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Take collected comments (consumes them)
    pub fn take_comments(&mut self) -> Vec<Comment> {
        std::mem::take(&mut self.comments)
    }

    /// Tokenize the entire source and return all tokens
    pub fn tokenize(&mut self) -> HoneResult<Vec<Token>> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    /// Get the next token
    pub fn next_token(&mut self) -> HoneResult<Token> {
        self.skip_whitespace_and_comments();

        self.token_start = self.position;
        self.token_start_line = self.line;
        self.token_start_column = self.column;

        match self.peek_char() {
            None => Ok(self.make_token(TokenKind::Eof)),
            Some(ch) => {
                match ch {
                    // Identifiers and keywords
                    'a'..='z' | 'A'..='Z' | '_' => self.lex_identifier(),

                    // Numbers
                    '0'..='9' => self.lex_number(),

                    // Minus operator or doc separator
                    '-' => {
                        let next = self.peek_char_at(1);
                        if matches!(next, Some('-')) {
                            // Check for --- (doc separator)
                            if matches!(self.peek_char_at(2), Some('-')) {
                                self.advance(); // -
                                self.advance(); // -
                                self.advance(); // -
                                Ok(self.make_token(TokenKind::DocSeparator))
                            } else {
                                self.advance();
                                Ok(self.make_token(TokenKind::Minus))
                            }
                        } else {
                            self.advance();
                            Ok(self.make_token(TokenKind::Minus))
                        }
                    }

                    // Strings
                    '"' => self.lex_double_string(),
                    '\'' => self.lex_single_string(),

                    // Punctuation and operators
                    '{' => {
                        self.advance();
                        // Track brace depth for interpolation
                        if self.interpolation_depth > 0 {
                            if let Some(depth) = self.interpolation_brace_stack.last_mut() {
                                *depth += 1;
                            }
                        }
                        Ok(self.make_token(TokenKind::LeftBrace))
                    }
                    '}' => {
                        // Check if we're closing an interpolation
                        if self.interpolation_depth > 0 {
                            if let Some(depth) = self.interpolation_brace_stack.last_mut() {
                                if *depth == 0 {
                                    // Check if this is a triple-string interpolation
                                    let is_triple = self
                                        .triple_string_interpolation
                                        .last()
                                        .copied()
                                        .unwrap_or(false);
                                    if is_triple {
                                        return self.continue_interpolated_triple_string();
                                    } else {
                                        return self.continue_interpolated_string();
                                    }
                                } else {
                                    *depth -= 1;
                                }
                            }
                        }
                        self.advance();
                        Ok(self.make_token(TokenKind::RightBrace))
                    }
                    '[' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::LeftBracket))
                    }
                    ']' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::RightBracket))
                    }
                    '(' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::LeftParen))
                    }
                    ')' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::RightParen))
                    }
                    ':' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Colon))
                    }
                    '+' => {
                        self.advance();
                        if self.peek_char() == Some(':') {
                            self.advance();
                            Ok(self.make_token(TokenKind::ColonPlus))
                        } else {
                            Ok(self.make_token(TokenKind::Plus))
                        }
                    }
                    '!' => {
                        self.advance();
                        if self.peek_char() == Some(':') {
                            self.advance();
                            Ok(self.make_token(TokenKind::ColonBang))
                        } else if self.peek_char() == Some('=') {
                            self.advance();
                            Ok(self.make_token(TokenKind::NotEq))
                        } else {
                            Ok(self.make_token(TokenKind::Not))
                        }
                    }
                    '@' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::At))
                    }
                    '.' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Dot))
                    }
                    ',' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Comma))
                    }
                    '*' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Star))
                    }
                    '/' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Slash))
                    }
                    '%' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Percent))
                    }
                    '=' => {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            Ok(self.make_token(TokenKind::EqEq))
                        } else {
                            Ok(self.make_token(TokenKind::Eq))
                        }
                    }
                    '<' => {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            Ok(self.make_token(TokenKind::LtEq))
                        } else {
                            Ok(self.make_token(TokenKind::Lt))
                        }
                    }
                    '>' => {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            Ok(self.make_token(TokenKind::GtEq))
                        } else {
                            Ok(self.make_token(TokenKind::Gt))
                        }
                    }
                    '&' => {
                        self.advance();
                        if self.peek_char() == Some('&') {
                            self.advance();
                            Ok(self.make_token(TokenKind::And))
                        } else {
                            Ok(self.make_token(TokenKind::Ampersand))
                        }
                    }
                    '|' => {
                        self.advance();
                        if self.peek_char() == Some('|') {
                            self.advance();
                            Ok(self.make_token(TokenKind::Or))
                        } else {
                            Ok(self.make_token(TokenKind::Pipe))
                        }
                    }
                    '?' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Question))
                    }
                    '\n' => {
                        self.advance();
                        Ok(self.make_token(TokenKind::Newline))
                    }

                    _ => {
                        let ch = self.advance().unwrap();
                        Err(self.error_unexpected_char(ch))
                    }
                }
            }
        }
    }

    /// Peek at the current character without consuming
    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    /// Peek at a character at offset from current position
    fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.source[self.position..].chars().nth(offset)
    }

    /// Advance to the next character
    fn advance(&mut self) -> Option<char> {
        if let Some((pos, ch)) = self.chars.next() {
            self.position = pos + ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek_char() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.advance();
                }
                Some('#') => {
                    // Line comment - collect it
                    let comment_line = self.line;
                    let comment_col = self.column;
                    self.advance(); // skip #
                                    // Skip optional space after #
                    let mut text = String::new();
                    while let Some(ch) = self.peek_char() {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                        text.push(ch);
                    }
                    self.comments.push(Comment {
                        text: text.trim_start_matches(' ').to_string(),
                        line: comment_line,
                        column: comment_col,
                        is_block: false,
                    });
                }
                Some('/') if self.peek_char_at(1) == Some('*') => {
                    // Block comment - collect it
                    let comment_line = self.line;
                    let comment_col = self.column;
                    self.advance(); // /
                    self.advance(); // *
                    let mut text = String::new();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.peek_char() {
                            None => break, // Unterminated block comment
                            Some('*') if self.peek_char_at(1) == Some('/') => {
                                self.advance();
                                self.advance();
                                depth -= 1;
                                if depth > 0 {
                                    text.push_str("*/");
                                }
                            }
                            Some('/') if self.peek_char_at(1) == Some('*') => {
                                self.advance();
                                self.advance();
                                depth += 1;
                                text.push_str("/*");
                            }
                            Some(ch) => {
                                self.advance();
                                text.push(ch);
                            }
                        }
                    }
                    self.comments.push(Comment {
                        text: text.trim().to_string(),
                        line: comment_line,
                        column: comment_col,
                        is_block: true,
                    });
                }
                _ => break,
            }
        }
    }

    /// Lex an identifier or keyword
    fn lex_identifier(&mut self) -> HoneResult<Token> {
        let start = self.position;

        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.source[start..self.position];

        let kind =
            TokenKind::keyword_from_str(text).unwrap_or_else(|| TokenKind::Ident(text.to_string()));

        Ok(self.make_token(kind))
    }

    /// Lex a number (integer or float)
    fn lex_number(&mut self) -> HoneResult<Token> {
        let start = self.position;
        let mut is_float = false;

        // Integer part
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        // Decimal part
        if self.peek_char() == Some('.') {
            if let Some(next) = self.peek_char_at(1) {
                if next.is_ascii_digit() {
                    is_float = true;
                    self.advance(); // .
                    while let Some(ch) = self.peek_char() {
                        if ch.is_ascii_digit() {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // Exponent part
        if let Some('e' | 'E') = self.peek_char() {
            is_float = true;
            self.advance();
            if let Some('+' | '-') = self.peek_char() {
                self.advance();
            }
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let text = &self.source[start..self.position];

        let kind = if is_float {
            let value: f64 = text.parse().map_err(|_| {
                HoneError::unexpected_token(
                    self.source.to_string(),
                    &self.current_location(),
                    "valid number",
                    text,
                    "invalid float literal",
                )
            })?;
            TokenKind::Float(value)
        } else {
            let value: i64 = text.parse().map_err(|_| {
                HoneError::unexpected_token(
                    self.source.to_string(),
                    &self.current_location(),
                    "valid number",
                    text,
                    "invalid integer literal",
                )
            })?;
            TokenKind::Integer(value)
        };

        Ok(self.make_token(kind))
    }

    /// Lex a double-quoted string (may contain interpolation)
    fn lex_double_string(&mut self) -> HoneResult<Token> {
        self.advance(); // opening "

        // Check for triple-quoted string
        if self.peek_char() == Some('"') && self.peek_char_at(1) == Some('"') {
            self.advance(); // "
            self.advance(); // "
            return self.lex_triple_string(true);
        }

        let mut value = String::new();
        let mut _has_interpolation = false;

        loop {
            match self.peek_char() {
                None | Some('\n') => {
                    return Err(HoneError::unterminated_string(
                        self.source.to_string(),
                        &self.token_location(),
                    ));
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    let escaped = self.lex_escape_sequence()?;
                    value.push(escaped);
                }
                Some('$') if self.peek_char_at(1) == Some('{') => {
                    // Start of interpolation
                    _has_interpolation = true;
                    self.advance(); // $
                    self.advance(); // {
                    self.interpolation_depth += 1;
                    self.interpolation_brace_stack.push(0);
                    self.triple_string_interpolation.push(false);

                    if value.is_empty() {
                        return Ok(self.make_token(TokenKind::StringStart(String::new())));
                    } else {
                        return Ok(self.make_token(TokenKind::StringStart(value)));
                    }
                }
                Some(ch) => {
                    self.advance();
                    value.push(ch);
                }
            }
        }

        Ok(self.make_token(TokenKind::String(value)))
    }

    /// Continue lexing an interpolated string after the expression
    fn continue_interpolated_string(&mut self) -> HoneResult<Token> {
        self.advance(); // closing }
        self.interpolation_depth -= 1;
        self.interpolation_brace_stack.pop();
        self.triple_string_interpolation.pop();

        self.token_start = self.position;
        self.token_start_line = self.line;
        self.token_start_column = self.column;

        let mut value = String::new();

        loop {
            match self.peek_char() {
                None | Some('\n') => {
                    return Err(HoneError::unterminated_string(
                        self.source.to_string(),
                        &self.token_location(),
                    ));
                }
                Some('"') => {
                    self.advance();
                    return Ok(self.make_token(TokenKind::StringEnd(value)));
                }
                Some('\\') => {
                    self.advance();
                    let escaped = self.lex_escape_sequence()?;
                    value.push(escaped);
                }
                Some('$') if self.peek_char_at(1) == Some('{') => {
                    // Another interpolation
                    self.advance(); // $
                    self.advance(); // {
                    self.interpolation_depth += 1;
                    self.interpolation_brace_stack.push(0);
                    self.triple_string_interpolation.push(false);

                    return Ok(self.make_token(TokenKind::StringMiddle(value)));
                }
                Some(ch) => {
                    self.advance();
                    value.push(ch);
                }
            }
        }
    }

    /// Continue lexing an interpolated triple-quoted string after the expression
    fn continue_interpolated_triple_string(&mut self) -> HoneResult<Token> {
        self.advance(); // closing }
        self.interpolation_depth -= 1;
        self.interpolation_brace_stack.pop();
        self.triple_string_interpolation.pop();

        self.token_start = self.position;
        self.token_start_line = self.line;
        self.token_start_column = self.column;

        let mut value = String::new();
        let mut consecutive_quotes = 0;

        loop {
            match self.peek_char() {
                None => {
                    return Err(HoneError::unterminated_string(
                        self.source.to_string(),
                        &self.token_location(),
                    ));
                }
                Some('"') => {
                    self.advance();
                    consecutive_quotes += 1;
                    if consecutive_quotes == 3 {
                        // End of triple string - remove the two quotes we added
                        value.pop();
                        value.pop();
                        return Ok(self.make_token(TokenKind::StringEnd(value)));
                    }
                    value.push('"');
                }
                Some('\\') => {
                    consecutive_quotes = 0;
                    self.advance();
                    let escaped = self.lex_escape_sequence()?;
                    value.push(escaped);
                }
                Some('$') if self.peek_char_at(1) == Some('{') => {
                    // Another interpolation
                    self.advance(); // $
                    self.advance(); // {
                    self.interpolation_depth += 1;
                    self.interpolation_brace_stack.push(0);
                    self.triple_string_interpolation.push(true);

                    return Ok(self.make_token(TokenKind::StringMiddle(value)));
                }
                Some(ch) => {
                    consecutive_quotes = 0;
                    self.advance();
                    value.push(ch);
                }
            }
        }
    }

    /// Lex a single-quoted string (no interpolation)
    fn lex_single_string(&mut self) -> HoneResult<Token> {
        self.advance(); // opening '

        // Check for triple-quoted string
        if self.peek_char() == Some('\'') && self.peek_char_at(1) == Some('\'') {
            self.advance(); // '
            self.advance(); // '
            return self.lex_triple_string(false);
        }

        let mut value = String::new();

        loop {
            match self.peek_char() {
                None | Some('\n') => {
                    return Err(HoneError::unterminated_string(
                        self.source.to_string(),
                        &self.token_location(),
                    ));
                }
                Some('\'') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    // Single-quoted strings: only \\ and \' are escapes
                    match self.peek_char() {
                        Some('\\') => {
                            self.advance();
                            value.push('\\');
                        }
                        Some('\'') => {
                            self.advance();
                            value.push('\'');
                        }
                        _ => {
                            value.push('\\');
                        }
                    }
                }
                Some(ch) => {
                    self.advance();
                    value.push(ch);
                }
            }
        }

        Ok(self.make_token(TokenKind::String(value)))
    }

    /// Lex a triple-quoted string
    fn lex_triple_string(&mut self, interpolate: bool) -> HoneResult<Token> {
        let mut value = String::new();
        let mut consecutive_quotes = 0;
        let quote_char = if interpolate { '"' } else { '\'' };

        // Skip initial newline if present
        if self.peek_char() == Some('\n') {
            self.advance();
        }

        loop {
            match self.peek_char() {
                None => {
                    return Err(HoneError::unterminated_string(
                        self.source.to_string(),
                        &self.token_location(),
                    ));
                }
                Some(ch) if ch == quote_char => {
                    self.advance();
                    consecutive_quotes += 1;
                    if consecutive_quotes == 3 {
                        // Remove the two quotes we added
                        value.pop();
                        value.pop();
                        break;
                    }
                    value.push(ch);
                }
                Some('\\') if interpolate => {
                    consecutive_quotes = 0;
                    self.advance();
                    let escaped = self.lex_escape_sequence()?;
                    value.push(escaped);
                }
                Some('$') if interpolate && self.peek_char_at(1) == Some('{') => {
                    // Start of interpolation in triple-quoted string
                    self.advance(); // $
                    self.advance(); // {
                    self.interpolation_depth += 1;
                    self.interpolation_brace_stack.push(0);
                    self.triple_string_interpolation.push(true);

                    if value.is_empty() {
                        return Ok(self.make_token(TokenKind::StringStart(String::new())));
                    } else {
                        return Ok(self.make_token(TokenKind::StringStart(value)));
                    }
                }
                Some(ch) => {
                    consecutive_quotes = 0;
                    self.advance();
                    value.push(ch);
                }
            }
        }

        // Strip leading indentation based on closing """ position
        let value = self.strip_triple_string_indent(&value);

        Ok(self.make_token(TokenKind::TripleString(value)))
    }

    /// Strip leading indentation from triple-quoted string
    fn strip_triple_string_indent(&self, s: &str) -> String {
        // Find minimum indentation (excluding empty lines)
        let min_indent = s
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        // Strip that much indentation from each line
        s.lines()
            .map(|line| {
                if line.len() >= min_indent {
                    &line[min_indent..]
                } else {
                    line.trim_start()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Lex an escape sequence
    fn lex_escape_sequence(&mut self) -> HoneResult<char> {
        match self.peek_char() {
            Some('n') => {
                self.advance();
                Ok('\n')
            }
            Some('r') => {
                self.advance();
                Ok('\r')
            }
            Some('t') => {
                self.advance();
                Ok('\t')
            }
            Some('\\') => {
                self.advance();
                Ok('\\')
            }
            Some('"') => {
                self.advance();
                Ok('"')
            }
            Some('\'') => {
                self.advance();
                Ok('\'')
            }
            Some('0') => {
                self.advance();
                Ok('\0')
            }
            Some('$') => {
                self.advance();
                Ok('$')
            }
            Some('{') => {
                self.advance();
                Ok('{')
            }
            Some('}') => {
                self.advance();
                Ok('}')
            }
            Some('u') => {
                // Unicode escape \u{XXXX}
                self.advance();
                if self.peek_char() != Some('{') {
                    return Err(HoneError::invalid_escape_sequence(
                        self.source.to_string(),
                        &self.current_location(),
                        "\\u",
                        "expected \\u{XXXX} for unicode escape",
                    ));
                }
                self.advance(); // {

                let mut hex = String::new();
                while let Some(ch) = self.peek_char() {
                    if ch == '}' {
                        break;
                    }
                    if !ch.is_ascii_hexdigit() {
                        return Err(HoneError::invalid_escape_sequence(
                            self.source.to_string(),
                            &self.current_location(),
                            format!("\\u{{{}", hex),
                            "invalid hex digit in unicode escape",
                        ));
                    }
                    hex.push(ch);
                    self.advance();
                }

                if self.peek_char() != Some('}') {
                    return Err(HoneError::invalid_escape_sequence(
                        self.source.to_string(),
                        &self.current_location(),
                        format!("\\u{{{}", hex),
                        "unterminated unicode escape",
                    ));
                }
                self.advance(); // }

                let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                    HoneError::invalid_escape_sequence(
                        self.source.to_string(),
                        &self.current_location(),
                        format!("\\u{{{}}}", hex),
                        "invalid unicode code point",
                    )
                })?;

                char::from_u32(code).ok_or_else(|| {
                    HoneError::invalid_escape_sequence(
                        self.source.to_string(),
                        &self.current_location(),
                        format!("\\u{{{}}}", hex),
                        "invalid unicode code point",
                    )
                })
            }
            Some(ch) => {
                let seq = format!("\\{}", ch);
                Err(HoneError::invalid_escape_sequence(
                    self.source.to_string(),
                    &self.current_location(),
                    seq.clone(),
                    format!(
                        "'{}' is not a valid escape sequence. Use '\\\\' for literal backslash",
                        seq
                    ),
                ))
            }
            None => Err(HoneError::invalid_escape_sequence(
                self.source.to_string(),
                &self.current_location(),
                "\\<eof>",
                "unexpected end of file in escape sequence",
            )),
        }
    }

    /// Create a token with the current token span
    fn make_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, self.token_location())
    }

    /// Get the location for the current token
    fn token_location(&self) -> SourceLocation {
        SourceLocation::new(
            self.file.clone(),
            self.token_start_line,
            self.token_start_column,
            self.token_start,
            self.position - self.token_start,
        )
    }

    /// Get the current location
    fn current_location(&self) -> SourceLocation {
        SourceLocation::new(self.file.clone(), self.line, self.column, self.position, 1)
    }

    /// Create an unexpected character error
    fn error_unexpected_char(&self, ch: char) -> HoneError {
        HoneError::unexpected_character(self.source.to_string(), &self.current_location(), ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source, None);
        lexer
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Newline))
            .collect()
    }

    #[test]
    fn test_empty() {
        assert_eq!(lex(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(lex("   \t  "), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_single_kv() {
        assert_eq!(
            lex("name: \"hello\""),
            vec![
                TokenKind::Ident("name".to_string()),
                TokenKind::Colon,
                TokenKind::String("hello".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_keywords() {
        assert_eq!(lex("let"), vec![TokenKind::Let, TokenKind::Eof]);
        assert_eq!(lex("from"), vec![TokenKind::From, TokenKind::Eof]);
        assert_eq!(lex("import"), vec![TokenKind::Import, TokenKind::Eof]);
        assert_eq!(lex("as"), vec![TokenKind::As, TokenKind::Eof]);
        assert_eq!(lex("when"), vec![TokenKind::When, TokenKind::Eof]);
        assert_eq!(lex("for"), vec![TokenKind::For, TokenKind::Eof]);
        assert_eq!(lex("in"), vec![TokenKind::In, TokenKind::Eof]);
        assert_eq!(lex("schema"), vec![TokenKind::Schema, TokenKind::Eof]);
        assert_eq!(lex("assert"), vec![TokenKind::Assert, TokenKind::Eof]);
        assert_eq!(lex("use"), vec![TokenKind::Use, TokenKind::Eof]);
        assert_eq!(lex("null"), vec![TokenKind::Null, TokenKind::Eof]);
        assert_eq!(lex("true"), vec![TokenKind::True, TokenKind::Eof]);
        assert_eq!(lex("false"), vec![TokenKind::False, TokenKind::Eof]);
    }

    #[test]
    fn test_identifier_with_hyphen() {
        assert_eq!(
            lex("my-service"),
            vec![TokenKind::Ident("my-service".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_numbers() {
        assert_eq!(lex("42"), vec![TokenKind::Integer(42), TokenKind::Eof]);
        // Negative numbers are lexed as Minus + Integer (parser handles unary minus)
        assert_eq!(
            lex("-17"),
            vec![TokenKind::Minus, TokenKind::Integer(17), TokenKind::Eof]
        );
        assert_eq!(lex("3.14"), vec![TokenKind::Float(3.14), TokenKind::Eof]);
        assert_eq!(
            lex("-2.5"),
            vec![TokenKind::Minus, TokenKind::Float(2.5), TokenKind::Eof]
        );
        assert_eq!(lex("1e10"), vec![TokenKind::Float(1e10), TokenKind::Eof]);
        assert_eq!(
            lex("1.5e-3"),
            vec![TokenKind::Float(1.5e-3), TokenKind::Eof]
        );
    }

    #[test]
    fn test_strings() {
        assert_eq!(
            lex("\"hello\""),
            vec![TokenKind::String("hello".to_string()), TokenKind::Eof]
        );
        assert_eq!(
            lex("'hello'"),
            vec![TokenKind::String("hello".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_string_escapes() {
        assert_eq!(
            lex("\"hello\\nworld\""),
            vec![
                TokenKind::String("hello\nworld".to_string()),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            lex("\"tab\\there\""),
            vec![TokenKind::String("tab\there".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_single_quote_literal() {
        // Single quotes don't interpret \n as escape
        assert_eq!(
            lex("'hello\\nworld'"),
            vec![
                TokenKind::String("hello\\nworld".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_string_interpolation() {
        let tokens = lex("\"hello ${name}\"");
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart("hello ".to_string()),
                TokenKind::Ident("name".to_string()),
                TokenKind::StringEnd(String::new()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_complex_interpolation() {
        let tokens = lex("\"prefix ${a + b} middle ${c} suffix\"");
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart("prefix ".to_string()),
                TokenKind::Ident("a".to_string()),
                TokenKind::Plus,
                TokenKind::Ident("b".to_string()),
                TokenKind::StringMiddle(" middle ".to_string()),
                TokenKind::Ident("c".to_string()),
                TokenKind::StringEnd(" suffix".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_punctuation() {
        assert_eq!(
            lex("{}[]():@.,"),
            vec![
                TokenKind::LeftBrace,
                TokenKind::RightBrace,
                TokenKind::LeftBracket,
                TokenKind::RightBracket,
                TokenKind::LeftParen,
                TokenKind::RightParen,
                TokenKind::Colon,
                TokenKind::At,
                TokenKind::Dot,
                TokenKind::Comma,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            lex("+: !:"),
            vec![TokenKind::ColonPlus, TokenKind::ColonBang, TokenKind::Eof,]
        );

        assert_eq!(
            lex("+ - * / %"),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Eof,
            ]
        );

        assert_eq!(
            lex("== != < > <= >="),
            vec![
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::Eof,
            ]
        );

        assert_eq!(
            lex("&& || !"),
            vec![
                TokenKind::And,
                TokenKind::Or,
                TokenKind::Not,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_doc_separator() {
        assert_eq!(lex("---"), vec![TokenKind::DocSeparator, TokenKind::Eof]);
        assert_eq!(
            lex("---deployment"),
            vec![
                TokenKind::DocSeparator,
                TokenKind::Ident("deployment".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(lex("# comment"), vec![TokenKind::Eof]);
        assert_eq!(
            lex("name # comment\nvalue"),
            vec![
                TokenKind::Ident("name".to_string()),
                TokenKind::Ident("value".to_string()),
                TokenKind::Eof,
            ]
        );
        assert_eq!(lex("/* block */"), vec![TokenKind::Eof]);
        assert_eq!(
            lex("a /* comment */ b"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_triple_quoted_string() {
        let tokens = lex("\"\"\"hello\nworld\"\"\"");
        assert!(matches!(tokens[0], TokenKind::TripleString(_)));
        if let TokenKind::TripleString(s) = &tokens[0] {
            assert!(s.contains("hello"));
            assert!(s.contains("world"));
        }
    }

    #[test]
    fn test_complete_example() {
        let source = r#"
let name = "test"

server {
  host: "localhost"
  port: 8080
}
"#;
        let tokens = lex(source);
        assert!(tokens.contains(&TokenKind::Let));
        assert!(tokens.contains(&TokenKind::Ident("name".to_string())));
        assert!(tokens.contains(&TokenKind::Eq));
        assert!(tokens.contains(&TokenKind::String("test".to_string())));
        assert!(tokens.contains(&TokenKind::LeftBrace));
        assert!(tokens.contains(&TokenKind::RightBrace));
    }

    #[test]
    fn test_unterminated_string_error() {
        let mut lexer = Lexer::new("\"unterminated", None);
        let result = lexer.tokenize();
        assert!(result.is_err());
        if let Err(HoneError::UnterminatedString { .. }) = result {
            // expected
        } else {
            panic!("Expected UnterminatedString error");
        }
    }

    #[test]
    fn test_source_locations() {
        let mut lexer = Lexer::new("name: 42", None);
        let tokens = lexer.tokenize().unwrap();

        // First token: "name" at line 1, column 1
        assert_eq!(tokens[0].location.line, 1);
        assert_eq!(tokens[0].location.column, 1);

        // Colon at line 1, column 5
        assert_eq!(tokens[1].location.line, 1);
        assert_eq!(tokens[1].location.column, 5);

        // Number at line 1, column 7
        assert_eq!(tokens[2].location.line, 1);
        assert_eq!(tokens[2].location.column, 7);
    }

    #[test]
    fn test_multiline_locations() {
        let mut lexer = Lexer::new("line1\nline2", None);
        let tokens = lexer.tokenize().unwrap();

        // First identifier at line 1
        assert_eq!(tokens[0].location.line, 1);

        // Newline token
        assert_eq!(tokens[1].kind, TokenKind::Newline);

        // Second identifier at line 2
        assert_eq!(tokens[2].location.line, 2);
        assert_eq!(tokens[2].location.column, 1);
    }

    #[test]
    fn test_triple_string_interpolation() {
        let tokens = lex("\"\"\"hello ${name}\"\"\"");
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart("hello ".to_string()),
                TokenKind::Ident("name".to_string()),
                TokenKind::StringEnd(String::new()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_triple_string_interpolation_multiline() {
        let src = "\"\"\"line1\n${x}\nline3\"\"\"";
        let tokens = lex(src);
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart("line1\n".to_string()),
                TokenKind::Ident("x".to_string()),
                TokenKind::StringEnd("\nline3".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_triple_string_multiple_interpolations() {
        let src = "\"\"\"a ${x} b ${y} c\"\"\"";
        let tokens = lex(src);
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart("a ".to_string()),
                TokenKind::Ident("x".to_string()),
                TokenKind::StringMiddle(" b ".to_string()),
                TokenKind::Ident("y".to_string()),
                TokenKind::StringEnd(" c".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_triple_string_no_interpolation_unchanged() {
        // Non-interpolated triple strings still work as before
        let tokens = lex("\"\"\"hello world\"\"\"");
        assert!(matches!(tokens[0], TokenKind::TripleString(_)));
        if let TokenKind::TripleString(s) = &tokens[0] {
            assert_eq!(s, "hello world");
        }
    }

    #[test]
    fn test_single_triple_string_no_interpolation() {
        // Single-quoted triple strings never support interpolation
        let tokens = lex("'''hello ${name}'''");
        assert!(matches!(tokens[0], TokenKind::TripleString(_)));
        if let TokenKind::TripleString(s) = &tokens[0] {
            assert_eq!(s, "hello ${name}");
        }
    }
}
