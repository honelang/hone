//! Parser for Hone configuration language
//!
//! This module implements a recursive descent parser that produces an AST
//! from a token stream. The parser is LL(1) with one token lookahead.

pub mod ast;

use crate::errors::{HoneError, HoneResult};
use crate::lexer::token::{SourceLocation, Token, TokenKind};
use ast::*;
use std::path::PathBuf;

/// Maximum parse recursion depth before the parser bails out.
/// Each nesting level expands to ~12 intermediate stack frames in the
/// recursive descent parser, so this must be conservative to avoid
/// stack overflow in debug builds.
const MAX_PARSE_DEPTH: usize = 128;

/// Parser for Hone source code
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    source: String,
    /// Current recursion depth
    depth: usize,
}

impl Parser {
    /// Create a new parser from a token stream
    pub fn new(tokens: Vec<Token>, source: impl Into<String>, _file: Option<PathBuf>) -> Self {
        Self {
            tokens,
            pos: 0,
            source: source.into(),
            depth: 0,
        }
    }

    /// Parse the entire file
    pub fn parse(&mut self) -> HoneResult<File> {
        let start_loc = self.current_location();
        let mut preamble = Vec::new();
        let mut body = Vec::new();
        let mut documents = Vec::new();

        // Skip leading newlines
        self.skip_newlines();

        // Parse preamble and body of main document
        self.parse_document_content(&mut preamble, &mut body)?;

        // Parse additional documents
        while self.check(&TokenKind::DocSeparator) {
            documents.push(self.parse_document()?);
        }

        // Expect EOF
        self.skip_newlines();
        if !self.is_at_end() {
            return Err(self.error_unexpected("end of file"));
        }

        let end_loc = self.current_location();
        Ok(File {
            preamble,
            body,
            documents,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse a named document after `---`
    fn parse_document(&mut self) -> HoneResult<Document> {
        let start_loc = self.current_location();

        // Consume `---`
        self.expect(&TokenKind::DocSeparator)?;

        // Optional document name
        let name = if let TokenKind::Ident(n) = &self.current().kind {
            let n = n.clone();
            self.advance();
            Some(n)
        } else {
            None
        };

        self.skip_newlines();

        let mut preamble = Vec::new();
        let mut body = Vec::new();

        self.parse_document_content(&mut preamble, &mut body)?;

        let end_loc = if body.is_empty() && preamble.is_empty() {
            start_loc.clone()
        } else {
            self.previous_location()
        };

        Ok(Document {
            name,
            preamble,
            body,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse the content of a document (preamble and body items)
    fn parse_document_content(
        &mut self,
        preamble: &mut Vec<PreambleItem>,
        body: &mut Vec<BodyItem>,
    ) -> HoneResult<()> {
        // First parse preamble items (let, from, import, schema, use)
        // Then switch to body items once we see body content
        let mut in_body = false;

        while !self.is_at_end() && !self.check(&TokenKind::DocSeparator) {
            self.skip_newlines();

            if self.is_at_end() || self.check(&TokenKind::DocSeparator) {
                break;
            }

            // Check what kind of item this is
            if !in_body && self.is_preamble_item() {
                preamble.push(self.parse_preamble_item()?);
            } else {
                in_body = true;
                body.push(self.parse_body_item()?);
            }

            self.skip_newlines();
        }

        Ok(())
    }

    /// Check if current position starts a preamble item
    fn is_preamble_item(&self) -> bool {
        match &self.current().kind {
            // These are always preamble items
            TokenKind::Let
            | TokenKind::From
            | TokenKind::Import
            | TokenKind::Use
            | TokenKind::Secret => true,
            // These are preamble items only if NOT followed by `:` (which would mean key usage)
            TokenKind::Schema
            | TokenKind::Type
            | TokenKind::Variant
            | TokenKind::Expect
            | TokenKind::Policy => {
                !self.peek_is(&TokenKind::Colon)
                    && !self.peek_is(&TokenKind::ColonPlus)
                    && !self.peek_is(&TokenKind::ColonBang)
            }
            _ => false,
        }
    }

    /// Parse a preamble item
    fn parse_preamble_item(&mut self) -> HoneResult<PreambleItem> {
        match &self.current().kind {
            TokenKind::Let => Ok(PreambleItem::Let(self.parse_let()?)),
            TokenKind::From => Ok(PreambleItem::From(self.parse_from()?)),
            TokenKind::Import => Ok(PreambleItem::Import(self.parse_import()?)),
            TokenKind::Schema => Ok(PreambleItem::Schema(self.parse_schema()?)),
            TokenKind::Type => Ok(PreambleItem::TypeAlias(self.parse_type_alias()?)),
            TokenKind::Use => Ok(PreambleItem::Use(self.parse_use()?)),
            TokenKind::Variant => Ok(PreambleItem::Variant(self.parse_variant()?)),
            TokenKind::Expect => Ok(PreambleItem::Expect(self.parse_expect()?)),
            TokenKind::Secret => Ok(PreambleItem::Secret(self.parse_secret()?)),
            TokenKind::Policy => Ok(PreambleItem::Policy(self.parse_policy()?)),
            _ => Err(self.error_unexpected("preamble item (let, from, import, schema, type, use, variant, expect, secret, policy)")),
        }
    }

    /// Parse a body item
    fn parse_body_item(&mut self) -> HoneResult<BodyItem> {
        // Check for stray comma (common mistake in block syntax)
        if self.check(&TokenKind::Comma) {
            let loc = self.current_location();
            return Err(HoneError::unexpected_token(
                &self.source,
                &loc,
                "key or block item",
                ",",
                "block syntax uses newlines to separate items, not commas. Use inline syntax for commas: key: { a: 1, b: 2 }",
            ));
        }
        match &self.current().kind {
            TokenKind::Let => Ok(BodyItem::Let(self.parse_let()?)),
            TokenKind::When => Ok(BodyItem::When(self.parse_when()?)),
            TokenKind::For => Ok(BodyItem::For(self.parse_for()?)),
            TokenKind::Assert => Ok(BodyItem::Assert(self.parse_assert()?)),
            TokenKind::Dot if self.peek_is(&TokenKind::Dot) => {
                // Spread: `...expr`
                Ok(BodyItem::Spread(self.parse_spread()?))
            }
            _ => {
                // Could be a key-value pair or a block
                self.parse_key_value_or_block()
            }
        }
    }

    /// Parse let binding: `let name = expr`
    fn parse_let(&mut self) -> HoneResult<LetBinding> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Let)?;

        let name = self.expect_ident("variable name")?;
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;

        let end_loc = value.location().clone();
        Ok(LetBinding {
            name,
            value,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse from statement: `from "path" [as alias]`
    fn parse_from(&mut self) -> HoneResult<FromStatement> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::From)?;

        let path = self.parse_string_expr()?;
        let alias = if self.check(&TokenKind::As) {
            self.advance();
            Some(self.expect_ident("alias name")?)
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(FromStatement {
            path,
            alias,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse import statement
    fn parse_import(&mut self) -> HoneResult<ImportStatement> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Import)?;

        let kind = if self.check(&TokenKind::LeftBrace) {
            // Named import: `import { a, b } from "path"`
            self.advance();
            let mut names = Vec::new();

            while !self.check(&TokenKind::RightBrace) {
                let name_loc = self.current_location();
                let name = self.expect_ident("import name")?;
                let alias = if self.check(&TokenKind::As) {
                    self.advance();
                    Some(self.expect_ident("alias name")?)
                } else {
                    None
                };
                let name_end = self.previous_location();
                names.push(ImportName {
                    name,
                    alias,
                    location: name_loc.span_to(&name_end),
                });

                if !self.check(&TokenKind::RightBrace) {
                    self.expect(&TokenKind::Comma)?;
                }
            }

            self.expect(&TokenKind::RightBrace)?;
            self.expect(&TokenKind::From)?;
            let path = self.parse_string_expr()?;

            ImportKind::Named { names, path }
        } else {
            // Whole import: `import "path" [as alias]`
            let path = self.parse_string_expr()?;
            let alias = if self.check(&TokenKind::As) {
                self.advance();
                Some(self.expect_ident("alias name")?)
            } else {
                None
            };

            ImportKind::Whole { path, alias }
        };

        let end_loc = self.previous_location();
        Ok(ImportStatement {
            kind,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse schema definition
    fn parse_schema(&mut self) -> HoneResult<SchemaDefinition> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Schema)?;

        let name = self.expect_ident("schema name")?;
        let extends = if self.check(&TokenKind::Extends) {
            self.advance();
            Some(self.expect_ident("base schema name")?)
        } else {
            None
        };

        self.expect(&TokenKind::LeftBrace)?;
        self.skip_separators();

        let mut fields = Vec::new();
        let mut open = false;
        while !self.check(&TokenKind::RightBrace) {
            // Check for `...` (open schema marker)
            if self.check(&TokenKind::Dot)
                && self.pos + 2 < self.tokens.len()
                && self.tokens[self.pos + 1].kind == TokenKind::Dot
                && self.tokens[self.pos + 2].kind == TokenKind::Dot
            {
                self.advance(); // .
                self.advance(); // .
                self.advance(); // .
                open = true;
                self.skip_separators();
                continue;
            }
            fields.push(self.parse_schema_field()?);
            self.skip_separators();
        }

        self.expect(&TokenKind::RightBrace)?;

        let end_loc = self.previous_location();
        Ok(SchemaDefinition {
            name,
            extends,
            fields,
            open,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse a schema field
    fn parse_schema_field(&mut self) -> HoneResult<SchemaField> {
        let start_loc = self.current_location();
        let name = self.expect_ident("field name")?;

        let optional = self.check(&TokenKind::Question);
        if optional {
            self.advance();
        }

        self.expect(&TokenKind::Colon)?;
        let constraint = self.parse_type_constraint()?;

        let default = if self.check(&TokenKind::Eq) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(SchemaField {
            name,
            constraint,
            optional,
            default,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse type constraint
    fn parse_type_constraint(&mut self) -> HoneResult<TypeConstraint> {
        let start_loc = self.current_location();
        let name = self.expect_ident("type name")?;

        let args = if self.check(&TokenKind::LeftParen) {
            self.advance();
            let mut args = Vec::new();

            while !self.check(&TokenKind::RightParen) {
                args.push(self.parse_expr()?);
                if !self.check(&TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }

            self.expect(&TokenKind::RightParen)?;
            args
        } else {
            Vec::new()
        };

        let end_loc = self.previous_location();
        Ok(TypeConstraint {
            name,
            args,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse type alias: `type Name = base_type & constraint1 & constraint2`
    fn parse_type_alias(&mut self) -> HoneResult<TypeAliasDefinition> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Type)?;

        let name = self.expect_ident("type alias name")?;
        self.expect(&TokenKind::Eq)?;

        let base_type = self.parse_type_expr()?;

        let end_loc = self.previous_location();
        Ok(TypeAliasDefinition {
            name,
            base_type,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse type expression: handles unions, optionals, and arrays
    fn parse_type_expr(&mut self) -> HoneResult<TypeExpr> {
        let mut expr = self.parse_type_primary()?;

        // Check for optional suffix
        if self.check(&TokenKind::Question) {
            self.advance();
            expr = TypeExpr::Optional(Box::new(expr));
        }

        // Check for union (| type2 | type3)
        if self.check(&TokenKind::Pipe) {
            let mut types = vec![expr];
            while self.check(&TokenKind::Pipe) {
                self.advance();
                types.push(self.parse_type_primary()?);
            }
            expr = TypeExpr::Union(types);
        }

        Ok(expr)
    }

    /// Parse primary type expression: name, name(args), or array<T>
    fn parse_type_primary(&mut self) -> HoneResult<TypeExpr> {
        let name = self.expect_ident("type name")?;

        // Check for array<T> syntax
        if self.check(&TokenKind::Lt) {
            self.advance();
            let elem_type = self.parse_type_expr()?;
            self.expect(&TokenKind::Gt)?;
            Ok(TypeExpr::Array(Box::new(elem_type)))
        } else if self.check(&TokenKind::LeftParen) {
            // Parse name(args) syntax like int(1, 65535)
            self.advance();
            let mut args = Vec::new();
            while !self.check(&TokenKind::RightParen) {
                args.push(self.parse_expr()?);
                if !self.check(&TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightParen)?;
            Ok(TypeExpr::Named { name, args })
        } else {
            Ok(TypeExpr::Named {
                name,
                args: Vec::new(),
            })
        }
    }

    /// Parse use statement: `use schema_name`
    fn parse_use(&mut self) -> HoneResult<UseStatement> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Use)?;

        let schema_name = self.expect_ident("schema name")?;

        let end_loc = self.previous_location();
        Ok(UseStatement {
            schema_name,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse variant definition: `variant name { [default] case_name { ... } ... }`
    fn parse_variant(&mut self) -> HoneResult<VariantDefinition> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Variant)?;

        let name = self.expect_ident("variant name")?;
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut cases = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let case_loc = self.current_location();

            // Check for `default` keyword (parsed as ident since it's not reserved)
            let is_default = if let TokenKind::Ident(id) = &self.current().kind {
                id == "default"
            } else {
                false
            };

            if is_default {
                self.advance(); // consume "default"
            }

            let case_name = self.expect_ident("variant case name")?;
            self.expect(&TokenKind::LeftBrace)?;
            self.skip_newlines();

            let mut body = Vec::new();
            while !self.check(&TokenKind::RightBrace) {
                body.push(self.parse_body_item()?);
                self.skip_newlines();
            }

            self.expect(&TokenKind::RightBrace)?;
            self.skip_newlines();

            let case_end = self.previous_location();
            cases.push(VariantCase {
                name: case_name,
                is_default,
                body,
                location: case_loc.span_to(&case_end),
            });
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_loc = self.previous_location();

        Ok(VariantDefinition {
            name,
            cases,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse expect declaration: `expect args.name: type [= default]`
    fn parse_expect(&mut self) -> HoneResult<ExpectDeclaration> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Expect)?;

        // Parse dotted path: args.name or args.server.port
        let mut path = Vec::new();
        path.push(self.expect_ident("expected path (e.g. args.env)")?);
        while self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.expect_ident("path segment")?);
        }

        // Expect colon
        self.expect(&TokenKind::Colon)?;

        // Parse type name (simple identifier: string, int, bool, float)
        let type_name = self.expect_ident("type name (string, int, bool, float)")?;

        // Optional default value
        let default = if self.check(&TokenKind::Eq) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(ExpectDeclaration {
            path,
            type_name,
            default,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse secret declaration: `secret name from "provider:path"`
    fn parse_secret(&mut self) -> HoneResult<SecretDeclaration> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Secret)?;

        let name = self.expect_ident("secret name")?;
        self.expect(&TokenKind::From)?;

        // Parse provider string (must be a plain string literal)
        let provider = match &self.current().kind {
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                return Err(self.error_unexpected(
                    "string literal for secret provider (e.g. \"vault:path\" or \"env:NAME\")",
                ));
            }
        };

        let end_loc = self.previous_location();
        Ok(SecretDeclaration {
            name,
            provider,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse policy declaration: `policy name deny/warn when condition { "message" }`
    fn parse_policy(&mut self) -> HoneResult<PolicyDeclaration> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Policy)?;

        let name = self.expect_ident("policy name")?;

        // Parse level: deny or warn
        let level = match &self.current().kind {
            TokenKind::Deny => {
                self.advance();
                PolicyLevel::Deny
            }
            TokenKind::Warn => {
                self.advance();
                PolicyLevel::Warn
            }
            _ => {
                return Err(self.error_unexpected("'deny' or 'warn'"));
            }
        };

        // Expect 'when' keyword
        self.expect(&TokenKind::When)?;

        // Parse condition expression
        let condition = self.parse_expr()?;

        // Parse optional message in braces
        let message = if self.check(&TokenKind::LeftBrace) {
            self.advance();
            self.skip_newlines();
            let msg = match &self.current().kind {
                TokenKind::String(s) => {
                    let s = s.clone();
                    self.advance();
                    Some(s)
                }
                _ => None,
            };
            self.skip_newlines();
            self.expect(&TokenKind::RightBrace)?;
            msg
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(PolicyDeclaration {
            name,
            level,
            condition,
            message,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse when block: `when condition { ... } [else when ... | else { ... }]`
    fn parse_when(&mut self) -> HoneResult<WhenBlock> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::When)?;

        let condition = self.parse_expr()?;
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            body.push(self.parse_body_item()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        // Check for else branch
        self.skip_newlines();
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            self.skip_newlines();
            if self.check(&TokenKind::When) {
                // else when condition { ... }
                let else_when = self.parse_when()?;
                Some(ElseBranch::ElseWhen(Box::new(else_when)))
            } else {
                // else { ... }
                let else_loc = self.current_location();
                self.expect(&TokenKind::LeftBrace)?;
                self.skip_newlines();
                let mut else_body = Vec::new();
                while !self.check(&TokenKind::RightBrace) {
                    else_body.push(self.parse_body_item()?);
                    self.skip_newlines();
                }
                self.expect(&TokenKind::RightBrace)?;
                let else_end = self.previous_location();
                Some(ElseBranch::Else(else_body, else_loc.span_to(&else_end)))
            }
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(WhenBlock {
            condition,
            body,
            else_branch,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse for loop: `for item in iterable { ... }`
    fn parse_for(&mut self) -> HoneResult<ForLoop> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::For)?;

        let binding = if self.check(&TokenKind::LeftParen) {
            // Destructuring: `for (k, v) in ...`
            self.advance();
            let first = self.expect_ident("first binding")?;
            self.expect(&TokenKind::Comma)?;
            let second = self.expect_ident("second binding")?;
            self.expect(&TokenKind::RightParen)?;
            ForBinding::Pair(first, second)
        } else {
            // Single: `for x in ...`
            let name = self.expect_ident("loop variable")?;
            ForBinding::Single(name)
        };

        self.expect(&TokenKind::In)?;
        let iterable = self.parse_expr()?;

        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        // Determine if this is an object body, block body, or expression body
        let body = if self.is_body_item_start() {
            let mut items = Vec::new();
            let mut trailing_expr = None;
            while !self.check(&TokenKind::RightBrace) {
                if self.is_body_item_start() {
                    items.push(self.parse_body_item()?);
                    self.skip_newlines();
                } else {
                    // Trailing expression after body items (block body)
                    trailing_expr = Some(self.parse_expr()?);
                    self.skip_newlines();
                    break;
                }
            }
            match trailing_expr {
                Some(expr) => ForBody::Block(items, expr),
                None => ForBody::Object(items),
            }
        } else {
            // Expression body
            let expr = self.parse_expr()?;
            self.skip_newlines();
            ForBody::Expr(expr)
        };

        self.expect(&TokenKind::RightBrace)?;

        let end_loc = self.previous_location();
        Ok(ForLoop {
            binding,
            iterable,
            body,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Check if current token starts a body item
    fn is_body_item_start(&self) -> bool {
        match &self.current().kind {
            TokenKind::Let | TokenKind::When | TokenKind::For | TokenKind::Assert => true,
            TokenKind::Ident(_) | TokenKind::String(_) => {
                // Could be a key-value or block
                // Look ahead for `:`, `+:`, `!:`, or `{`
                if self.pos + 1 < self.tokens.len() {
                    matches!(
                        self.tokens[self.pos + 1].kind,
                        TokenKind::Colon
                            | TokenKind::ColonPlus
                            | TokenKind::ColonBang
                            | TokenKind::LeftBrace
                    )
                } else {
                    false
                }
            }
            TokenKind::StringStart(_) => {
                // Interpolated string key: `"${expr}": value`
                // Scan past the interpolated string to check if followed by `:`, `+:`, `!:`, or `{`
                if let Some(end_pos) = self.find_matching_string_end(self.pos) {
                    if end_pos + 1 < self.tokens.len() {
                        matches!(
                            self.tokens[end_pos + 1].kind,
                            TokenKind::Colon
                                | TokenKind::ColonPlus
                                | TokenKind::ColonBang
                                | TokenKind::LeftBrace
                        )
                    } else {
                        false
                    }
                } else {
                    true // fallback: assume it's a key (old behavior)
                }
            }
            _ => false,
        }
    }

    /// Find the position of the StringEnd token matching a StringStart at `start_pos`.
    /// Handles nested interpolated strings.
    fn find_matching_string_end(&self, start_pos: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut pos = start_pos;
        while pos < self.tokens.len() {
            match &self.tokens[pos].kind {
                TokenKind::StringStart(_) => depth += 1,
                TokenKind::StringEnd(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
                _ => {}
            }
            pos += 1;
        }
        None
    }

    /// Parse assert statement: `assert condition [: message]`
    fn parse_assert(&mut self) -> HoneResult<AssertStatement> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::Assert)?;

        let condition = self.parse_expr()?;
        let message = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let end_loc = self.previous_location();
        Ok(AssertStatement {
            condition,
            message,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse spread expression: `...expr`
    fn parse_spread(&mut self) -> HoneResult<SpreadExpr> {
        let start_loc = self.current_location();

        // Consume three dots
        self.expect(&TokenKind::Dot)?;
        self.expect(&TokenKind::Dot)?;
        self.expect(&TokenKind::Dot)?;

        let expr = self.parse_expr()?;
        let end_loc = expr.location().clone();

        Ok(SpreadExpr {
            expr,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse key-value pair or block
    fn parse_key_value_or_block(&mut self) -> HoneResult<BodyItem> {
        let start_loc = self.current_location();

        // Parse the key
        let key = self.parse_key()?;

        // Check for block syntax: `name { ... }`
        if let Key::Ident(name) = &key {
            if self.check(&TokenKind::LeftBrace) {
                let name = name.clone();
                self.advance();
                self.skip_newlines();

                let mut items = Vec::new();
                while !self.check(&TokenKind::RightBrace) {
                    items.push(self.parse_body_item()?);
                    self.skip_newlines();
                }

                self.expect(&TokenKind::RightBrace)?;
                let end_loc = self.previous_location();

                return Ok(BodyItem::Block(Block {
                    name,
                    items,
                    location: start_loc.span_to(&end_loc),
                }));
            }
        }

        // Parse assignment operator
        let op = self.parse_assign_op()?;

        // Parse value
        let value = self.parse_expr()?;
        let end_loc = value.location().clone();

        Ok(BodyItem::KeyValue(KeyValue {
            key,
            op,
            value,
            location: start_loc.span_to(&end_loc),
        }))
    }

    /// Parse a key
    fn parse_key(&mut self) -> HoneResult<Key> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Key::Ident(name))
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Key::String(s))
            }
            TokenKind::StringStart(_) => {
                // Interpolated string key: `"${expr}": value`
                let expr = self.parse_expr()?;
                Ok(Key::Computed(Box::new(expr)))
            }
            TokenKind::LeftBracket => {
                // Computed key: `[expr]`
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightBracket)?;
                Ok(Key::Computed(Box::new(expr)))
            }
            kind if kind.is_keyword() => {
                let keyword = format!("{}", kind);
                let loc = self.current_location();
                Err(HoneError::ReservedWordAsKey {
                    src: self.source.clone(),
                    span: (loc.offset, loc.length).into(),
                    keyword,
                })
            }
            _ => Err(self.error_unexpected("key (identifier, string, or [expr])")),
        }
    }

    /// Parse assignment operator
    fn parse_assign_op(&mut self) -> HoneResult<AssignOp> {
        match &self.current().kind {
            TokenKind::Colon => {
                self.advance();
                Ok(AssignOp::Colon)
            }
            TokenKind::ColonPlus => {
                self.advance();
                Ok(AssignOp::Append)
            }
            TokenKind::ColonBang => {
                self.advance();
                Ok(AssignOp::Replace)
            }
            _ => Err(self.error_unexpected("assignment operator (:, +:, or !:)")),
        }
    }

    /// Parse an expression
    fn parse_expr(&mut self) -> HoneResult<Expr> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            let loc = self.current_location();
            self.depth -= 1;
            return Err(HoneError::RecursionLimitExceeded {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                help: format!(
                    "expression nesting exceeds maximum depth of {}; simplify your configuration",
                    MAX_PARSE_DEPTH
                ),
            });
        }
        let result = self.parse_conditional();
        self.depth -= 1;
        result
    }

    /// Parse conditional expression: `a ? b : c`
    fn parse_conditional(&mut self) -> HoneResult<Expr> {
        let start_loc = self.current_location();
        let condition = self.parse_or()?;

        if self.check(&TokenKind::Question) {
            self.advance();
            let then_branch = self.parse_expr()?;
            self.expect(&TokenKind::Colon)?;
            let else_branch = self.parse_expr()?;
            let end_loc = else_branch.location().clone();

            Ok(Expr::Conditional(ConditionalExpr {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
                location: start_loc.span_to(&end_loc),
            }))
        } else {
            Ok(condition)
        }
    }

    /// Parse OR expression: `a || b`
    fn parse_or(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_and()?;

        while self.check(&TokenKind::Or) {
            let start_loc = left.location().clone();
            self.advance();
            let right = self.parse_and()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse AND expression: `a && b`
    fn parse_and(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_equality()?;

        while self.check(&TokenKind::And) {
            let start_loc = left.location().clone();
            self.advance();
            let right = self.parse_equality()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse equality expression: `a == b`, `a != b`
    fn parse_equality(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_comparison()?;

        while matches!(self.current().kind, TokenKind::EqEq | TokenKind::NotEq) {
            let start_loc = left.location().clone();
            let op = match &self.current().kind {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::NotEq,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_comparison()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse comparison expression: `a < b`, `a > b`, etc.
    fn parse_comparison(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_null_coalesce()?;

        while matches!(
            self.current().kind,
            TokenKind::Lt | TokenKind::Gt | TokenKind::LtEq | TokenKind::GtEq
        ) {
            let start_loc = left.location().clone();
            let op = match &self.current().kind {
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::LtEq => BinaryOp::LtEq,
                TokenKind::GtEq => BinaryOp::GtEq,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_null_coalesce()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse null coalescing: `a ?? b`
    fn parse_null_coalesce(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_additive()?;

        while self.check(&TokenKind::Question) && self.peek_is(&TokenKind::Question) {
            let start_loc = left.location().clone();
            self.advance(); // first ?
            self.advance(); // second ?
            let right = self.parse_additive()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::NullCoalesce,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse additive expression: `a + b`, `a - b`
    fn parse_additive(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_multiplicative()?;

        while matches!(self.current().kind, TokenKind::Plus | TokenKind::Minus) {
            let start_loc = left.location().clone();
            let op = match &self.current().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse multiplicative expression: `a * b`, `a / b`, `a % b`
    fn parse_multiplicative(&mut self) -> HoneResult<Expr> {
        let mut left = self.parse_unary()?;

        while matches!(
            self.current().kind,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent
        ) {
            let start_loc = left.location().clone();
            let op = match &self.current().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_unary()?;
            let end_loc = right.location().clone();

            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op,
                right: Box::new(right),
                location: start_loc.span_to(&end_loc),
            });
        }

        Ok(left)
    }

    /// Parse unary expression: `!a`, `-a`
    fn parse_unary(&mut self) -> HoneResult<Expr> {
        if matches!(self.current().kind, TokenKind::Not | TokenKind::Minus) {
            let start_loc = self.current_location();
            let op = match &self.current().kind {
                TokenKind::Not => UnaryOp::Not,
                TokenKind::Minus => UnaryOp::Neg,
                _ => unreachable!(),
            };
            self.advance();
            let operand = self.parse_unary()?;
            let end_loc = operand.location().clone();

            Ok(Expr::Unary(UnaryExpr {
                op,
                operand: Box::new(operand),
                location: start_loc.span_to(&end_loc),
            }))
        } else {
            self.parse_postfix()
        }
    }

    /// Parse postfix expression: function calls, indexing, member access
    fn parse_postfix(&mut self) -> HoneResult<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            match &self.current().kind {
                TokenKind::LeftParen => {
                    // Function call
                    let start_loc = expr.location().clone();
                    self.advance();
                    let mut args = Vec::new();

                    while !self.check(&TokenKind::RightParen) {
                        args.push(self.parse_expr()?);
                        if !self.check(&TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }

                    self.expect(&TokenKind::RightParen)?;
                    let end_loc = self.previous_location();

                    expr = Expr::Call(CallExpr {
                        func: Box::new(expr),
                        args,
                        location: start_loc.span_to(&end_loc),
                    });
                }
                TokenKind::LeftBracket => {
                    // Index access
                    let start_loc = expr.location().clone();
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&TokenKind::RightBracket)?;
                    let end_loc = self.previous_location();

                    expr = Expr::Index(IndexExpr {
                        base: Box::new(expr),
                        index: Box::new(index),
                        location: start_loc.span_to(&end_loc),
                    });
                }
                TokenKind::Dot => {
                    // Member access - convert to path expression
                    let start_loc = expr.location().clone();
                    let mut parts = match expr {
                        Expr::Ident(name, _) => vec![PathPart::Ident(name)],
                        Expr::Path(path) => path.parts,
                        _ => {
                            // Create a path from a more complex expression
                            // This is a simplification - full impl would need more work
                            return Err(HoneError::unexpected_token(
                                self.source.clone(),
                                &self.current_location(),
                                "identifier",
                                ".",
                                "member access requires an identifier base",
                            ));
                        }
                    };

                    while self.check(&TokenKind::Dot) {
                        self.advance();
                        let name = self.expect_ident("member name")?;
                        parts.push(PathPart::Ident(name));
                    }

                    let end_loc = self.previous_location();
                    expr = Expr::Path(PathExpr {
                        parts,
                        location: start_loc.span_to(&end_loc),
                    });
                }
                TokenKind::At => {
                    // Type annotation
                    let start_loc = expr.location().clone();
                    self.advance();
                    let constraint = self.parse_type_constraint()?;
                    let end_loc = self.previous_location();

                    expr = Expr::Annotated(AnnotatedExpr {
                        expr: Box::new(expr),
                        constraint,
                        location: start_loc.span_to(&end_loc),
                    });
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse primary expression
    fn parse_primary(&mut self) -> HoneResult<Expr> {
        let start_loc = self.current_location();

        match &self.current().kind.clone() {
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null(start_loc))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true, start_loc))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false, start_loc))
            }
            TokenKind::Integer(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Integer(n, start_loc))
            }
            TokenKind::Float(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Float(n, start_loc))
            }
            TokenKind::String(_) | TokenKind::StringStart(_) | TokenKind::TripleString(_) => {
                Ok(Expr::String(self.parse_string_expr()?))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expr::Ident(name, start_loc))
            }
            TokenKind::LeftBracket => {
                // Array literal
                self.parse_array()
            }
            TokenKind::LeftBrace => {
                // Object literal
                self.parse_object()
            }
            TokenKind::LeftParen => {
                // Parenthesized expression
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_loc = self.previous_location();
                Ok(Expr::Paren(Box::new(expr), start_loc.span_to(&end_loc)))
            }
            TokenKind::For => {
                // For expression (in array context)
                let for_loop = self.parse_for()?;
                Ok(Expr::For(Box::new(for_loop)))
            }
            TokenKind::When => {
                // When expression (in array/object context)
                let when_block = self.parse_when()?;
                Ok(Expr::When(Box::new(when_block)))
            }
            _ => Err(self.error_unexpected("expression")),
        }
    }

    /// Parse a string expression (possibly with interpolations)
    fn parse_string_expr(&mut self) -> HoneResult<StringExpr> {
        let start_loc = self.current_location();
        let mut parts = Vec::new();

        match &self.current().kind.clone() {
            TokenKind::String(s) => {
                parts.push(StringPart::Literal(s.clone()));
                self.advance();
            }
            TokenKind::TripleString(s) => {
                parts.push(StringPart::Literal(s.clone()));
                self.advance();
            }
            TokenKind::StringStart(s) => {
                parts.push(StringPart::Literal(s.clone()));
                self.advance();

                // Parse interpolated expression
                let expr = self.parse_expr()?;
                parts.push(StringPart::Interpolation(expr));

                // Continue parsing string parts
                loop {
                    match &self.current().kind.clone() {
                        TokenKind::StringMiddle(s) => {
                            parts.push(StringPart::Literal(s.clone()));
                            self.advance();
                            let expr = self.parse_expr()?;
                            parts.push(StringPart::Interpolation(expr));
                        }
                        TokenKind::StringEnd(s) => {
                            parts.push(StringPart::Literal(s.clone()));
                            self.advance();
                            break;
                        }
                        _ => {
                            return Err(self.error_unexpected("string continuation or end"));
                        }
                    }
                }
            }
            _ => {
                return Err(self.error_unexpected("string"));
            }
        }

        let end_loc = self.previous_location();
        Ok(StringExpr {
            parts,
            location: start_loc.span_to(&end_loc),
        })
    }

    /// Parse array literal
    fn parse_array(&mut self) -> HoneResult<Expr> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::LeftBracket)?;
        self.skip_newlines();

        let mut elements = Vec::new();

        while !self.check(&TokenKind::RightBracket) {
            // Check for spread, for, when, or regular expression
            if self.check(&TokenKind::Dot) && self.peek_is(&TokenKind::Dot) {
                // Spread
                self.advance(); // first .
                self.advance(); // second .
                self.expect(&TokenKind::Dot)?; // third .
                let expr = self.parse_expr()?;
                elements.push(ArrayElement::Spread(expr));
            } else if self.check(&TokenKind::For) {
                let for_loop = self.parse_for()?;
                elements.push(ArrayElement::For(for_loop));
            } else if self.check(&TokenKind::When) {
                let when_block = self.parse_when()?;
                elements.push(ArrayElement::When(when_block));
            } else {
                let expr = self.parse_expr()?;
                elements.push(ArrayElement::Expr(expr));
            }

            self.skip_newlines();
            if !self.check(&TokenKind::RightBracket) {
                if self.check(&TokenKind::Comma) {
                    self.advance();
                }
                self.skip_newlines();
            }
        }

        self.expect(&TokenKind::RightBracket)?;
        let end_loc = self.previous_location();

        Ok(Expr::Array(ArrayExpr {
            elements,
            location: start_loc.span_to(&end_loc),
        }))
    }

    /// Parse object literal
    fn parse_object(&mut self) -> HoneResult<Expr> {
        let start_loc = self.current_location();
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_separators();

        let mut items = Vec::new();

        while !self.check(&TokenKind::RightBrace) {
            items.push(self.parse_body_item()?);
            self.skip_separators();
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_loc = self.previous_location();

        Ok(Expr::Object(ObjectExpr {
            items,
            location: start_loc.span_to(&end_loc),
        }))
    }

    /// Skip newlines and commas (separators between items)
    fn skip_separators(&mut self) {
        while matches!(self.current().kind, TokenKind::Newline | TokenKind::Comma) {
            self.advance();
        }
    }

    // Helper methods

    /// Get the current token
    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("token stream should not be empty")
        })
    }

    /// Get the current token's location
    fn current_location(&self) -> SourceLocation {
        self.current().location.clone()
    }

    /// Get the previous token's location
    fn previous_location(&self) -> SourceLocation {
        if self.pos > 0 {
            self.tokens[self.pos - 1].location.clone()
        } else {
            self.current_location()
        }
    }

    /// Check if we're at the end of input
    fn is_at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    /// Check if current token matches expected kind
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    /// Check if next token matches expected kind
    fn peek_is(&self, kind: &TokenKind) -> bool {
        if self.pos + 1 < self.tokens.len() {
            std::mem::discriminant(&self.tokens[self.pos + 1].kind) == std::mem::discriminant(kind)
        } else {
            false
        }
    }

    /// Advance to next token
    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    /// Skip newline tokens
    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    /// Expect a specific token kind
    fn expect(&mut self, kind: &TokenKind) -> HoneResult<()> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_unexpected(&format!("{}", kind)))
        }
    }

    /// Expect an identifier and return its name
    fn expect_ident(&mut self, context: &str) -> HoneResult<String> {
        if let TokenKind::Ident(name) = &self.current().kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(self.error_unexpected(context))
        }
    }

    /// Create an "unexpected token" error
    fn error_unexpected(&self, expected: &str) -> HoneError {
        HoneError::unexpected_token(
            self.source.clone(),
            &self.current_location(),
            expected,
            format!("{}", self.current().kind),
            "check syntax",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> HoneResult<File> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        parser.parse()
    }

    #[test]
    fn test_empty_file() {
        let file = parse("").unwrap();
        assert!(file.preamble.is_empty());
        assert!(file.body.is_empty());
        assert!(file.documents.is_empty());
    }

    #[test]
    fn test_simple_key_value() {
        let file = parse("name: \"hello\"").unwrap();
        assert!(file.preamble.is_empty());
        assert_eq!(file.body.len(), 1);

        if let BodyItem::KeyValue(kv) = &file.body[0] {
            assert!(matches!(&kv.key, Key::Ident(s) if s == "name"));
            assert!(matches!(&kv.op, AssignOp::Colon));
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_let_binding() {
        let file = parse("let x = 42").unwrap();
        assert_eq!(file.preamble.len(), 1);

        if let PreambleItem::Let(binding) = &file.preamble[0] {
            assert_eq!(binding.name, "x");
            assert!(matches!(&binding.value, Expr::Integer(42, _)));
        } else {
            panic!("expected let binding");
        }
    }

    #[test]
    fn test_from_statement() {
        let file = parse("from \"./base.hone\"").unwrap();
        assert_eq!(file.preamble.len(), 1);

        if let PreambleItem::From(from) = &file.preamble[0] {
            assert_eq!(from.parts_as_string(), "./base.hone");
            assert!(from.alias.is_none());
        } else {
            panic!("expected from statement");
        }
    }

    #[test]
    fn test_from_with_alias() {
        let file = parse("from \"./base.hone\" as base").unwrap();
        assert_eq!(file.preamble.len(), 1);

        if let PreambleItem::From(from) = &file.preamble[0] {
            assert_eq!(from.alias, Some("base".to_string()));
        } else {
            panic!("expected from statement");
        }
    }

    #[test]
    fn test_block() {
        let file = parse("server { host: \"localhost\" }").unwrap();
        assert_eq!(file.body.len(), 1);

        if let BodyItem::Block(block) = &file.body[0] {
            assert_eq!(block.name, "server");
            assert_eq!(block.items.len(), 1);
        } else {
            panic!("expected block");
        }
    }

    #[test]
    fn test_when_block() {
        let file = parse("when x == 1 { a: 1 }").unwrap();
        assert_eq!(file.body.len(), 1);

        if let BodyItem::When(when) = &file.body[0] {
            assert_eq!(when.body.len(), 1);
        } else {
            panic!("expected when block");
        }
    }

    #[test]
    fn test_for_loop() {
        let file = parse("items: [for x in [1, 2, 3] { x }]").unwrap();
        assert_eq!(file.body.len(), 1);

        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Array(arr) = &kv.value {
                assert_eq!(arr.elements.len(), 1);
                assert!(matches!(&arr.elements[0], ArrayElement::For(_)));
            } else {
                panic!("expected array");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_arithmetic_expr() {
        let file = parse("x: 1 + 2 * 3").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            // Should parse as 1 + (2 * 3) due to precedence
            if let Expr::Binary(bin) = &kv.value {
                assert!(matches!(bin.op, BinaryOp::Add));
                assert!(matches!(&*bin.right, Expr::Binary(b) if b.op == BinaryOp::Mul));
            } else {
                panic!("expected binary expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_comparison_expr() {
        let file = parse("valid: x > 0 && x < 100").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Binary(bin) = &kv.value {
                assert!(matches!(bin.op, BinaryOp::And));
            } else {
                panic!("expected binary expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_conditional_expr() {
        let file = parse("x: a ? 1 : 2").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            assert!(matches!(&kv.value, Expr::Conditional(_)));
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_function_call() {
        let file = parse("x: len(items)").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Call(call) = &kv.value {
                assert_eq!(call.args.len(), 1);
            } else {
                panic!("expected call expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_array_literal() {
        let file = parse("arr: [1, 2, 3]").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Array(arr) = &kv.value {
                assert_eq!(arr.elements.len(), 3);
            } else {
                panic!("expected array");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_object_literal() {
        let file = parse("obj: { a: 1, b: 2 }").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Object(obj) = &kv.value {
                assert_eq!(obj.items.len(), 2);
            } else {
                panic!("expected object");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_type_annotation() {
        let file = parse("port: 8080 @int(1, 65535)").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Annotated(ann) = &kv.value {
                assert_eq!(ann.constraint.name, "int");
                assert_eq!(ann.constraint.args.len(), 2);
            } else {
                panic!("expected annotated expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_multi_document() {
        let file = parse("a: 1\n---deployment\nb: 2\n---service\nc: 3").unwrap();
        assert_eq!(file.documents.len(), 2);
        assert_eq!(file.documents[0].name, Some("deployment".to_string()));
        assert_eq!(file.documents[1].name, Some("service".to_string()));
    }

    #[test]
    fn test_append_operator() {
        let file = parse("items +: [1]").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            assert!(matches!(kv.op, AssignOp::Append));
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_replace_operator() {
        let file = parse("items !: [1]").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            assert!(matches!(kv.op, AssignOp::Replace));
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_path_expr() {
        let file = parse("x: a.b.c").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Path(path) = &kv.value {
                assert_eq!(path.parts.len(), 3);
            } else {
                panic!("expected path expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_string_interpolation() {
        let file = parse("msg: \"hello ${name}\"").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::String(s) = &kv.value {
                assert_eq!(s.parts.len(), 3); // "hello ", interpolation, ""
            } else {
                panic!("expected string expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_schema_definition() {
        let file = parse("schema User { name: string, age?: int = 0 }").unwrap();
        assert_eq!(file.preamble.len(), 1);

        if let PreambleItem::Schema(schema) = &file.preamble[0] {
            assert_eq!(schema.name, "User");
            assert_eq!(schema.fields.len(), 2);
            assert_eq!(schema.fields[0].name, "name");
            assert!(!schema.fields[0].optional);
            assert_eq!(schema.fields[1].name, "age");
            assert!(schema.fields[1].optional);
            assert!(schema.fields[1].default.is_some());
        } else {
            panic!("expected schema");
        }
    }

    #[test]
    fn test_schema_extends() {
        let file = parse("schema Admin extends User { role: string }").unwrap();
        if let PreambleItem::Schema(schema) = &file.preamble[0] {
            assert_eq!(schema.extends, Some("User".to_string()));
        } else {
            panic!("expected schema");
        }
    }

    #[test]
    fn test_import_whole() {
        let file = parse("import \"./utils.hone\" as utils").unwrap();
        if let PreambleItem::Import(import) = &file.preamble[0] {
            if let ImportKind::Whole { alias, .. } = &import.kind {
                assert_eq!(*alias, Some("utils".to_string()));
            } else {
                panic!("expected whole import");
            }
        } else {
            panic!("expected import");
        }
    }

    #[test]
    fn test_import_named() {
        let file = parse("import { foo, bar as baz } from \"./lib.hone\"").unwrap();
        if let PreambleItem::Import(import) = &file.preamble[0] {
            if let ImportKind::Named { names, .. } = &import.kind {
                assert_eq!(names.len(), 2);
                assert_eq!(names[0].name, "foo");
                assert!(names[0].alias.is_none());
                assert_eq!(names[1].name, "bar");
                assert_eq!(names[1].alias, Some("baz".to_string()));
            } else {
                panic!("expected named import");
            }
        } else {
            panic!("expected import");
        }
    }

    #[test]
    fn test_unary_not() {
        let file = parse("enabled: !disabled").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Unary(u) = &kv.value {
                assert!(matches!(u.op, UnaryOp::Not));
            } else {
                panic!("expected unary expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_unary_neg() {
        let file = parse("delta: -5").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Unary(u) = &kv.value {
                assert!(matches!(u.op, UnaryOp::Neg));
            } else {
                panic!("expected unary expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_index_expr() {
        let file = parse("x: items[0]").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Index(idx) = &kv.value {
                assert!(matches!(&*idx.index, Expr::Integer(0, _)));
            } else {
                panic!("expected index expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_nested_blocks() {
        let file = parse("server { config { debug: true } }").unwrap();
        if let BodyItem::Block(outer) = &file.body[0] {
            assert_eq!(outer.name, "server");
            if let BodyItem::Block(inner) = &outer.items[0] {
                assert_eq!(inner.name, "config");
            } else {
                panic!("expected inner block");
            }
        } else {
            panic!("expected block");
        }
    }

    #[test]
    fn test_assert_statement() {
        let file = parse("assert x > 0: \"x must be positive\"").unwrap();
        if let BodyItem::Assert(a) = &file.body[0] {
            assert!(a.message.is_some());
        } else {
            panic!("expected assert");
        }
    }

    #[test]
    fn test_use_statement() {
        let file = parse("use MySchema").unwrap();
        if let PreambleItem::Use(u) = &file.preamble[0] {
            assert_eq!(u.schema_name, "MySchema");
        } else {
            panic!("expected use statement");
        }
    }

    #[test]
    fn test_for_destructuring() {
        let file = parse("items: [for (k, v) in map { k }]").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Array(arr) = &kv.value {
                if let ArrayElement::For(f) = &arr.elements[0] {
                    assert!(matches!(&f.binding, ForBinding::Pair(_, _)));
                } else {
                    panic!("expected for element");
                }
            } else {
                panic!("expected array");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_paren_expr() {
        let file = parse("x: (1 + 2) * 3").unwrap();
        if let BodyItem::KeyValue(kv) = &file.body[0] {
            if let Expr::Binary(b) = &kv.value {
                assert!(matches!(b.op, BinaryOp::Mul));
                assert!(matches!(&*b.left, Expr::Paren(_, _)));
            } else {
                panic!("expected binary expr");
            }
        } else {
            panic!("expected key-value");
        }
    }

    #[test]
    fn test_complete_example_parses() {
        let source = r#"
let env = "production"
let base_port = 8000

server {
  host: "localhost"
  port: base_port + 1
  name: "api-${env}"
  ssl: true
  timeout_ms: 30000

  config {
    debug: false
    log_level: "info"
  }
}

ports: [80, 443, 8080]

when env == "production" {
  replicas: 3
}

containers: [
  for i in [1, 2, 3] {
    name: "worker-${i}"
  }
]

port: 8080 @int(1, 65535)

---deployment
apiVersion: "apps/v1"
kind: "Deployment"

---service
apiVersion: "v1"
kind: "Service"
"#;
        let file = parse(source).unwrap();
        assert_eq!(file.preamble.len(), 2); // let env, let base_port
        assert!(file.body.len() >= 4); // server, ports, when, containers, port
        assert_eq!(file.documents.len(), 2); // deployment, service
    }

    #[test]
    fn test_type_alias_simple() {
        let source = "type Port = int";
        let file = parse(source).unwrap();
        assert_eq!(file.preamble.len(), 1);
        if let PreambleItem::TypeAlias(alias) = &file.preamble[0] {
            assert_eq!(alias.name, "Port");
            if let TypeExpr::Named { name, args } = &alias.base_type {
                assert_eq!(name, "int");
                assert!(args.is_empty());
            } else {
                panic!("expected named type");
            }
        } else {
            panic!("expected type alias");
        }
    }

    #[test]
    fn test_type_alias_with_constraint() {
        // New unified syntax: int(min, max)
        let source = "type Port = int(1, 65535)";
        let file = parse(source).unwrap();
        assert_eq!(file.preamble.len(), 1);
        if let PreambleItem::TypeAlias(alias) = &file.preamble[0] {
            assert_eq!(alias.name, "Port");
            if let TypeExpr::Named { name, args } = &alias.base_type {
                assert_eq!(name, "int");
                assert_eq!(args.len(), 2);
                // args[0] should be 1, args[1] should be 65535
                if let Expr::Integer(min, _) = &args[0] {
                    assert_eq!(*min, 1);
                } else {
                    panic!("expected integer for min");
                }
                if let Expr::Integer(max, _) = &args[1] {
                    assert_eq!(*max, 65535);
                } else {
                    panic!("expected integer for max");
                }
            } else {
                panic!("expected named type with args");
            }
        } else {
            panic!("expected type alias");
        }
    }

    #[test]
    fn test_type_alias_optional() {
        let source = "type OptionalPort = int?";
        let file = parse(source).unwrap();
        if let PreambleItem::TypeAlias(alias) = &file.preamble[0] {
            assert!(matches!(alias.base_type, TypeExpr::Optional(_)));
        } else {
            panic!("expected type alias");
        }
    }

    #[test]
    fn test_type_alias_array() {
        let source = "type Ports = array<int>";
        let file = parse(source).unwrap();
        if let PreambleItem::TypeAlias(alias) = &file.preamble[0] {
            if let TypeExpr::Array(inner) = &alias.base_type {
                if let TypeExpr::Named { name, args } = inner.as_ref() {
                    assert_eq!(name, "int");
                    assert!(args.is_empty());
                } else {
                    panic!("expected named type");
                }
            } else {
                panic!("expected array type");
            }
        } else {
            panic!("expected type alias");
        }
    }

    #[test]
    fn test_type_alias_union() {
        let source = "type StringOrInt = string | int";
        let file = parse(source).unwrap();
        if let PreambleItem::TypeAlias(alias) = &file.preamble[0] {
            if let TypeExpr::Union(types) = &alias.base_type {
                assert_eq!(types.len(), 2);
            } else {
                panic!("expected union type");
            }
        } else {
            panic!("expected type alias");
        }
    }
}
