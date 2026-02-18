//! Source code formatter for the Hone configuration language
//!
//! Formats Hone source code according to canonical style rules:
//! - 2-space indentation
//! - Opening brace on same line
//! - One blank line between preamble and body
//! - Preserves comments
//! - Idempotent (formatting twice produces the same result)

use crate::errors::HoneResult;
use crate::lexer::{Comment, Lexer};
use crate::parser::ast::*;
use crate::parser::Parser;

/// Format Hone source code and return the formatted string.
pub fn format_source(source: &str) -> HoneResult<String> {
    let mut lexer = Lexer::new(source, None);
    let tokens = lexer.tokenize()?;
    let comments = lexer.take_comments();

    let mut parser = Parser::new(tokens, source, None);
    let ast = parser.parse()?;

    let mut formatter = Formatter::new(comments);
    formatter.format_file(&ast);
    Ok(formatter.finish())
}

/// Formatter state
struct Formatter {
    /// Output buffer
    output: String,
    /// Current indentation level
    indent: usize,
    /// Collected comments from lexer
    comments: Vec<Comment>,
    /// Index into comments array (next comment to emit)
    comment_idx: usize,
    /// Track the last emitted line (for comment placement)
    current_line: usize,
}

impl Formatter {
    fn new(comments: Vec<Comment>) -> Self {
        Self {
            output: String::new(),
            indent: 0,
            comments,
            comment_idx: 0,
            current_line: 1,
        }
    }

    fn finish(mut self) -> String {
        // Emit any remaining comments
        self.emit_remaining_comments();
        // Ensure single trailing newline
        let trimmed = self.output.trim_end().to_string();
        if trimmed.is_empty() {
            return String::new();
        }
        trimmed + "\n"
    }

    fn emit_remaining_comments(&mut self) {
        while self.comment_idx < self.comments.len() {
            let comment = self.comments[self.comment_idx].clone();
            self.comment_idx += 1;
            self.write_indent();
            if comment.is_block {
                self.output.push_str(&format!("/* {} */", comment.text));
            } else {
                self.output.push_str(&format!("# {}", comment.text));
            }
            self.output.push('\n');
        }
    }

    /// Emit comments that appear before the given source line
    fn emit_comments_before(&mut self, line: usize) {
        while self.comment_idx < self.comments.len() {
            let comment = &self.comments[self.comment_idx];
            if comment.line >= line {
                break;
            }
            let comment = comment.clone();
            self.comment_idx += 1;
            self.write_indent();
            if comment.is_block {
                self.output.push_str(&format!("/* {} */", comment.text));
            } else {
                self.output.push_str(&format!("# {}", comment.text));
            }
            self.output.push('\n');
            self.current_line = comment.line + 1;
        }
    }

    /// Emit an inline comment on the same line, if any
    fn emit_inline_comment(&mut self, line: usize) {
        if self.comment_idx < self.comments.len() {
            let comment = &self.comments[self.comment_idx];
            if comment.line == line {
                let comment = comment.clone();
                self.comment_idx += 1;
                if comment.is_block {
                    self.output.push_str(&format!(" /* {} */", comment.text));
                } else {
                    self.output.push_str(&format!(" # {}", comment.text));
                }
            }
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    fn format_file(&mut self, file: &File) {
        let has_preamble = !file.preamble.is_empty();
        let has_body = !file.body.is_empty();

        // Format preamble
        for (i, item) in file.preamble.iter().enumerate() {
            let line = self.preamble_item_line(item);
            self.emit_comments_before(line);

            self.format_preamble_item(item);

            // Add blank line between different preamble item types
            if i + 1 < file.preamble.len() {
                let next = &file.preamble[i + 1];
                if self.preamble_needs_blank_line(item, next) {
                    self.output.push('\n');
                }
            }
        }

        // Blank line between preamble and body
        if has_preamble && has_body {
            self.output.push('\n');
        }

        // Format body
        self.format_body_items(&file.body);

        // Format sub-documents
        for doc in &file.documents {
            self.output.push('\n');
            self.output.push_str("---");
            if let Some(ref name) = doc.name {
                self.output.push_str(name);
            }
            self.output.push('\n');

            for item in &doc.preamble {
                let line = self.preamble_item_line(item);
                self.emit_comments_before(line);
                self.format_preamble_item(item);
            }

            if !doc.preamble.is_empty() && !doc.body.is_empty() {
                self.output.push('\n');
            }

            self.format_body_items(&doc.body);
        }
    }

    fn preamble_item_line(&self, item: &PreambleItem) -> usize {
        match item {
            PreambleItem::Let(b) => b.location.line,
            PreambleItem::From(f) => f.location.line,
            PreambleItem::Import(i) => i.location.line,
            PreambleItem::Schema(s) => s.location.line,
            PreambleItem::TypeAlias(t) => t.location.line,
            PreambleItem::Use(u) => u.location.line,
            PreambleItem::Variant(v) => v.location.line,
            PreambleItem::Expect(e) => e.location.line,
            PreambleItem::Secret(s) => s.location.line,
            PreambleItem::Policy(p) => p.location.line,
        }
    }

    fn body_item_line(&self, item: &BodyItem) -> usize {
        match item {
            BodyItem::KeyValue(kv) => kv.location.line,
            BodyItem::Block(b) => b.location.line,
            BodyItem::When(w) => w.location.line,
            BodyItem::For(f) => f.location.line,
            BodyItem::Assert(a) => a.location.line,
            BodyItem::Let(l) => l.location.line,
            BodyItem::Spread(s) => s.location.line,
        }
    }

    /// Check if we should add a blank line between two preamble items
    fn preamble_needs_blank_line(&self, current: &PreambleItem, next: &PreambleItem) -> bool {
        // Blank line between different kinds of preamble items
        std::mem::discriminant(current) != std::mem::discriminant(next)
    }

    fn format_preamble_item(&mut self, item: &PreambleItem) {
        match item {
            PreambleItem::Let(binding) => {
                self.write_indent();
                self.output.push_str("let ");
                self.output.push_str(&binding.name);
                self.output.push_str(" = ");
                self.format_expr(&binding.value);
                self.emit_inline_comment(binding.location.line);
                self.output.push('\n');
            }
            PreambleItem::From(from) => {
                self.write_indent();
                self.output.push_str("from ");
                self.format_string_expr(&from.path);
                if let Some(ref alias) = from.alias {
                    self.output.push_str(" as ");
                    self.output.push_str(alias);
                }
                self.emit_inline_comment(from.location.line);
                self.output.push('\n');
            }
            PreambleItem::Import(import) => {
                self.write_indent();
                match &import.kind {
                    ImportKind::Whole { path, alias } => {
                        self.output.push_str("import ");
                        self.format_string_expr(path);
                        if let Some(ref alias) = alias {
                            self.output.push_str(" as ");
                            self.output.push_str(alias);
                        }
                    }
                    ImportKind::Named { names, path } => {
                        self.output.push_str("import { ");
                        for (i, name) in names.iter().enumerate() {
                            if i > 0 {
                                self.output.push_str(", ");
                            }
                            self.output.push_str(&name.name);
                            if let Some(ref alias) = name.alias {
                                self.output.push_str(" as ");
                                self.output.push_str(alias);
                            }
                        }
                        self.output.push_str(" } from ");
                        self.format_string_expr(path);
                    }
                }
                self.emit_inline_comment(import.location.line);
                self.output.push('\n');
            }
            PreambleItem::Schema(schema) => {
                self.write_indent();
                self.output.push_str("schema ");
                self.output.push_str(&schema.name);
                if let Some(ref extends) = schema.extends {
                    self.output.push_str(" extends ");
                    self.output.push_str(extends);
                }
                self.output.push_str(" {\n");
                self.indent += 1;
                for field in &schema.fields {
                    self.emit_comments_before(field.location.line);
                    self.write_indent();
                    self.output.push_str(&field.name);
                    if field.optional {
                        self.output.push('?');
                    }
                    self.output.push_str(": ");
                    self.format_type_constraint(&field.constraint);
                    self.emit_inline_comment(field.location.line);
                    self.output.push('\n');
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            PreambleItem::TypeAlias(alias) => {
                self.write_indent();
                self.output.push_str("type ");
                self.output.push_str(&alias.name);
                self.output.push_str(" = ");
                self.format_type_expr(&alias.base_type);
                self.emit_inline_comment(alias.location.line);
                self.output.push('\n');
            }
            PreambleItem::Use(use_stmt) => {
                self.write_indent();
                self.output.push_str("use ");
                self.output.push_str(&use_stmt.schema_name);
                self.emit_inline_comment(use_stmt.location.line);
                self.output.push('\n');
            }
            PreambleItem::Variant(variant) => {
                self.write_indent();
                self.output.push_str("variant ");
                self.output.push_str(&variant.name);
                self.output.push_str(" {\n");
                self.indent += 1;
                for (i, case) in variant.cases.iter().enumerate() {
                    if i > 0 {
                        self.output.push('\n');
                    }
                    self.emit_comments_before(case.location.line);
                    self.write_indent();
                    if case.is_default {
                        self.output.push_str("default ");
                    }
                    self.output.push_str(&case.name);
                    self.output.push_str(" {\n");
                    self.indent += 1;
                    self.format_body_items(&case.body);
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push_str("}\n");
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            PreambleItem::Expect(expect) => {
                self.write_indent();
                self.output.push_str("expect ");
                self.output.push_str(&expect.path.join("."));
                self.output.push_str(": ");
                self.output.push_str(&expect.type_name);
                if let Some(ref default) = expect.default {
                    self.output.push_str(" = ");
                    self.format_expr(default);
                }
                self.emit_inline_comment(expect.location.line);
                self.output.push('\n');
            }
            PreambleItem::Secret(secret) => {
                self.write_indent();
                self.output.push_str("secret ");
                self.output.push_str(&secret.name);
                self.output.push_str(" from \"");
                self.output.push_str(&secret.provider);
                self.output.push('"');
                self.emit_inline_comment(secret.location.line);
                self.output.push('\n');
            }
            PreambleItem::Policy(policy) => {
                self.write_indent();
                self.output.push_str("policy ");
                self.output.push_str(&policy.name);
                self.output.push(' ');
                match policy.level {
                    crate::parser::ast::PolicyLevel::Deny => self.output.push_str("deny"),
                    crate::parser::ast::PolicyLevel::Warn => self.output.push_str("warn"),
                }
                self.output.push_str(" when ");
                self.format_expr(&policy.condition);
                if let Some(ref msg) = policy.message {
                    self.output.push_str(" {\n");
                    self.indent += 1;
                    self.write_indent();
                    self.output.push('"');
                    self.output.push_str(msg);
                    self.output.push('"');
                    self.output.push('\n');
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push('}');
                }
                self.emit_inline_comment(policy.location.line);
                self.output.push('\n');
            }
        }
    }

    fn format_type_constraint(&mut self, constraint: &TypeConstraint) {
        self.output.push_str(&constraint.name);
        if !constraint.args.is_empty() {
            self.output.push('(');
            for (i, arg) in constraint.args.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.format_expr(arg);
            }
            self.output.push(')');
        }
    }

    fn format_type_expr(&mut self, expr: &TypeExpr) {
        match expr {
            TypeExpr::Named { name, args } => {
                self.output.push_str(name);
                if !args.is_empty() {
                    self.output.push('(');
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.format_expr(arg);
                    }
                    self.output.push(')');
                }
            }
            TypeExpr::Array(inner) => {
                self.output.push_str("array<");
                self.format_type_expr(inner);
                self.output.push('>');
            }
            TypeExpr::Optional(inner) => {
                self.format_type_expr(inner);
                self.output.push('?');
            }
            TypeExpr::Union(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(" | ");
                    }
                    self.format_type_expr(t);
                }
            }
        }
    }

    fn format_body_items(&mut self, items: &[BodyItem]) {
        for (i, item) in items.iter().enumerate() {
            let line = self.body_item_line(item);
            self.emit_comments_before(line);
            self.format_body_item(item);

            // Add blank line before blocks/when if not already at start
            if i + 1 < items.len() {
                let next = &items[i + 1];
                if self.body_needs_blank_line(item, next) {
                    self.output.push('\n');
                }
            }
        }
    }

    fn body_needs_blank_line(&self, current: &BodyItem, next: &BodyItem) -> bool {
        // Blank line before/after blocks and when blocks
        matches!(current, BodyItem::Block(_) | BodyItem::When(_))
            || matches!(next, BodyItem::Block(_) | BodyItem::When(_))
    }

    fn format_body_item(&mut self, item: &BodyItem) {
        match item {
            BodyItem::KeyValue(kv) => {
                self.write_indent();
                self.format_key(&kv.key);
                match kv.op {
                    AssignOp::Colon => self.output.push_str(": "),
                    AssignOp::Append => self.output.push_str(" +: "),
                    AssignOp::Replace => self.output.push_str(" !: "),
                };
                self.format_expr(&kv.value);
                self.emit_inline_comment(kv.location.line);
                self.output.push('\n');
            }
            BodyItem::Block(block) => {
                self.write_indent();
                self.output.push_str(&block.name);
                self.output.push_str(" {\n");
                self.indent += 1;
                self.format_body_items(&block.items);
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            BodyItem::When(when) => {
                self.write_indent();
                self.format_when_block(when);
                self.output.push('\n');
            }
            BodyItem::For(for_loop) => {
                self.write_indent();
                self.format_for_loop(for_loop);
                self.output.push('\n');
            }
            BodyItem::Assert(assert) => {
                self.write_indent();
                self.output.push_str("assert ");
                self.format_expr(&assert.condition);
                if let Some(ref msg) = assert.message {
                    self.output.push_str(" : ");
                    self.format_expr(msg);
                }
                self.emit_inline_comment(assert.location.line);
                self.output.push('\n');
            }
            BodyItem::Let(binding) => {
                self.write_indent();
                self.output.push_str("let ");
                self.output.push_str(&binding.name);
                self.output.push_str(" = ");
                self.format_expr(&binding.value);
                self.emit_inline_comment(binding.location.line);
                self.output.push('\n');
            }
            BodyItem::Spread(spread) => {
                self.write_indent();
                self.output.push_str("...");
                self.format_expr(&spread.expr);
                self.emit_inline_comment(spread.location.line);
                self.output.push('\n');
            }
        }
    }

    fn format_key(&mut self, key: &Key) {
        match key {
            Key::Ident(name) => self.output.push_str(name),
            Key::String(s) => {
                self.output.push('"');
                self.output.push_str(&escape_string(s));
                self.output.push('"');
            }
            Key::Computed(expr) => {
                self.output.push('[');
                self.format_expr(expr);
                self.output.push(']');
            }
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Null(_) => self.output.push_str("null"),
            Expr::Bool(b, _) => {
                self.output.push_str(if *b { "true" } else { "false" });
            }
            Expr::Integer(n, _) => {
                self.output.push_str(&n.to_string());
            }
            Expr::Float(n, _) => {
                let s = format!("{}", n);
                self.output.push_str(&s);
                // Ensure there's a decimal point
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    self.output.push_str(".0");
                }
            }
            Expr::String(s) => {
                self.format_string_expr(s);
            }
            Expr::Ident(name, _) => {
                self.output.push_str(name);
            }
            Expr::Path(path) => {
                for (i, part) in path.parts.iter().enumerate() {
                    match part {
                        PathPart::Ident(name) => {
                            if i > 0 {
                                self.output.push('.');
                            }
                            self.output.push_str(name);
                        }
                        PathPart::Index(expr) => {
                            self.output.push('[');
                            self.format_expr(expr);
                            self.output.push(']');
                        }
                    }
                }
            }
            Expr::Array(arr) => {
                self.format_array(arr);
            }
            Expr::Object(obj) => {
                self.format_inline_object(obj);
            }
            Expr::Binary(bin) => {
                self.format_expr(&bin.left);
                let op_str = match bin.op {
                    BinaryOp::Add => " + ",
                    BinaryOp::Sub => " - ",
                    BinaryOp::Mul => " * ",
                    BinaryOp::Div => " / ",
                    BinaryOp::Mod => " % ",
                    BinaryOp::Eq => " == ",
                    BinaryOp::NotEq => " != ",
                    BinaryOp::Lt => " < ",
                    BinaryOp::Gt => " > ",
                    BinaryOp::LtEq => " <= ",
                    BinaryOp::GtEq => " >= ",
                    BinaryOp::And => " && ",
                    BinaryOp::Or => " || ",
                    BinaryOp::NullCoalesce => " ?? ",
                };
                self.output.push_str(op_str);
                self.format_expr(&bin.right);
            }
            Expr::Unary(unary) => {
                match unary.op {
                    UnaryOp::Not => self.output.push('!'),
                    UnaryOp::Neg => self.output.push('-'),
                }
                self.format_expr(&unary.operand);
            }
            Expr::Call(call) => {
                self.format_expr(&call.func);
                self.output.push('(');
                for (i, arg) in call.args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.output.push(')');
            }
            Expr::Index(idx) => {
                self.format_expr(&idx.base);
                self.output.push('[');
                self.format_expr(&idx.index);
                self.output.push(']');
            }
            Expr::Conditional(cond) => {
                self.format_expr(&cond.condition);
                self.output.push_str(" ? ");
                self.format_expr(&cond.then_branch);
                self.output.push_str(" : ");
                self.format_expr(&cond.else_branch);
            }
            Expr::Annotated(ann) => {
                self.format_expr(&ann.expr);
                self.output.push_str(" @");
                self.format_type_constraint(&ann.constraint);
            }
            Expr::Paren(inner, _) => {
                self.output.push('(');
                self.format_expr(inner);
                self.output.push(')');
            }
            Expr::For(for_loop) => {
                self.format_for_loop(for_loop);
            }
            Expr::When(when) => {
                self.format_when_inline(when);
            }
        }
    }

    fn format_string_expr(&mut self, s: &StringExpr) {
        // Check if it's a simple string (no interpolation)
        if s.parts.len() == 1 {
            if let StringPart::Literal(text) = &s.parts[0] {
                self.output.push('"');
                self.output.push_str(&escape_string(text));
                self.output.push('"');
                return;
            }
        }

        // Interpolated string
        self.output.push('"');
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => {
                    self.output.push_str(&escape_string(text));
                }
                StringPart::Interpolation(expr) => {
                    self.output.push_str("${");
                    self.format_expr(expr);
                    self.output.push('}');
                }
            }
        }
        self.output.push('"');
    }

    fn format_array(&mut self, arr: &ArrayExpr) {
        if arr.elements.is_empty() {
            self.output.push_str("[]");
            return;
        }

        // Check if we can format inline (short, simple elements)
        let inline = self.can_format_array_inline(arr);

        if inline {
            self.output.push('[');
            for (i, elem) in arr.elements.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.format_array_element(elem);
            }
            self.output.push(']');
        } else {
            self.output.push_str("[\n");
            self.indent += 1;
            for elem in &arr.elements {
                self.write_indent();
                self.format_array_element(elem);
                self.output.push_str(",\n");
            }
            self.indent -= 1;
            self.write_indent();
            self.output.push(']');
        }
    }

    fn can_format_array_inline(&self, arr: &ArrayExpr) -> bool {
        // Inline if all elements are simple and total is short
        if arr.elements.len() > 6 {
            return false;
        }
        for elem in &arr.elements {
            match elem {
                ArrayElement::Expr(e) => {
                    if !self.is_simple_expr(e) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    fn is_simple_expr(&self, expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Null(_)
                | Expr::Bool(_, _)
                | Expr::Integer(_, _)
                | Expr::Float(_, _)
                | Expr::Ident(_, _)
        ) || matches!(expr, Expr::String(s) if s.parts.len() == 1 && matches!(&s.parts[0], StringPart::Literal(t) if t.len() < 30))
            || matches!(expr, Expr::Unary(u) if matches!(u.op, UnaryOp::Neg) && matches!(u.operand.as_ref(), Expr::Integer(_, _) | Expr::Float(_, _)))
    }

    fn format_array_element(&mut self, elem: &ArrayElement) {
        match elem {
            ArrayElement::Expr(e) => self.format_expr(e),
            ArrayElement::Spread(e) => {
                self.output.push_str("...");
                self.format_expr(e);
            }
            ArrayElement::For(for_loop) => {
                self.format_for_loop(for_loop);
            }
            ArrayElement::When(when) => {
                self.format_when_inline(when);
            }
        }
    }

    /// Format a when block with optional else chain (body-level, multiline)
    fn format_when_block(&mut self, when: &WhenBlock) {
        self.output.push_str("when ");
        self.format_expr(&when.condition);
        self.output.push_str(" {\n");
        self.indent += 1;
        self.format_body_items(&when.body);
        self.indent -= 1;
        self.write_indent();
        self.output.push('}');
        if let Some(ref else_branch) = when.else_branch {
            match else_branch {
                ElseBranch::ElseWhen(else_when) => {
                    self.output.push_str(" else ");
                    self.format_when_block(else_when);
                }
                ElseBranch::Else(else_body, _) => {
                    self.output.push_str(" else {\n");
                    self.indent += 1;
                    self.format_body_items(else_body);
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push('}');
                }
            }
        }
    }

    /// Format a when block inline (for array/object expression contexts)
    fn format_when_inline(&mut self, when: &WhenBlock) {
        self.output.push_str("when ");
        self.format_expr(&when.condition);
        self.output.push_str(" { ");
        for item in &when.body {
            self.format_body_item_inline(item);
        }
        self.output.push_str(" }");
        if let Some(ref else_branch) = when.else_branch {
            match else_branch {
                ElseBranch::ElseWhen(else_when) => {
                    self.output.push_str(" else ");
                    self.format_when_inline(else_when);
                }
                ElseBranch::Else(else_body, _) => {
                    self.output.push_str(" else { ");
                    for item in else_body {
                        self.format_body_item_inline(item);
                    }
                    self.output.push_str(" }");
                }
            }
        }
    }

    fn format_for_loop(&mut self, for_loop: &ForLoop) {
        self.output.push_str("for ");
        match &for_loop.binding {
            ForBinding::Single(name) => self.output.push_str(name),
            ForBinding::Pair(k, v) => {
                self.output.push('(');
                self.output.push_str(k);
                self.output.push_str(", ");
                self.output.push_str(v);
                self.output.push(')');
            }
        }
        self.output.push_str(" in ");
        self.format_expr(&for_loop.iterable);
        self.output.push(' ');

        match &for_loop.body {
            ForBody::Expr(e) => {
                self.output.push_str("{ ");
                self.format_expr(e);
                self.output.push_str(" }");
            }
            ForBody::Object(items) => {
                // Check if all items are simple key-values
                if items.len() <= 2 && items.iter().all(|i| matches!(i, BodyItem::KeyValue(_))) {
                    self.output.push_str("{ ");
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.format_body_item_inline(item);
                    }
                    self.output.push_str(" }");
                } else {
                    self.output.push_str("{\n");
                    self.indent += 1;
                    self.format_body_items(items);
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push('}');
                }
            }
            ForBody::Block(items, expr) => {
                self.output.push_str("{\n");
                self.indent += 1;
                self.format_body_items(items);
                self.write_indent();
                self.format_expr(expr);
                self.output.push('\n');
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
            }
        }
    }

    fn format_inline_object(&mut self, obj: &ObjectExpr) {
        if obj.items.is_empty() {
            self.output.push_str("{}");
            return;
        }

        // Inline if short and all items are simple key-values
        let all_simple = obj
            .items
            .iter()
            .all(|i| matches!(i, BodyItem::KeyValue(kv) if self.is_simple_expr(&kv.value)));

        if all_simple && obj.items.len() <= 4 {
            self.output.push_str("{ ");
            for (i, item) in obj.items.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.format_body_item_inline(item);
            }
            self.output.push_str(" }");
        } else {
            self.output.push_str("{\n");
            self.indent += 1;
            self.format_body_items(&obj.items);
            self.indent -= 1;
            self.write_indent();
            self.output.push('}');
        }
    }

    /// Format a body item inline (no newline, no indent)
    fn format_body_item_inline(&mut self, item: &BodyItem) {
        match item {
            BodyItem::KeyValue(kv) => {
                self.format_key(&kv.key);
                match kv.op {
                    AssignOp::Colon => self.output.push_str(": "),
                    AssignOp::Append => self.output.push_str(" +: "),
                    AssignOp::Replace => self.output.push_str(" !: "),
                };
                self.format_expr(&kv.value);
            }
            BodyItem::Spread(spread) => {
                self.output.push_str("...");
                self.format_expr(&spread.expr);
            }
            _ => {
                // For other items in inline context, fall back to normal
                self.format_body_item(item);
            }
        }
    }
}

/// Escape a string for output in double quotes
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            c => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_simple() {
        let source = "name:   \"hello\"\nport:8080\n";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "name: \"hello\"\nport: 8080\n");
    }

    #[test]
    fn test_format_let_bindings() {
        let source = "let   x  =  42\nlet y = \"hello\"\nvalue: x\n";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "let x = 42\nlet y = \"hello\"\n\nvalue: x\n");
    }

    #[test]
    fn test_format_block() {
        let source = "server{host:\"localhost\"\nport:8080}";
        let formatted = format_source(source).unwrap();
        assert_eq!(
            formatted,
            "server {\n  host: \"localhost\"\n  port: 8080\n}\n"
        );
    }

    #[test]
    fn test_format_nested_blocks() {
        let source = "server{config{debug:true}}";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "server {\n  config {\n    debug: true\n  }\n}\n");
    }

    #[test]
    fn test_format_preserves_comments() {
        let source = "# This is a comment\nname: \"hello\"\n";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("# This is a comment"));
        assert!(formatted.contains("name: \"hello\""));
    }

    #[test]
    fn test_format_idempotent() {
        let source = r#"
let env = "production"
let port = 8080

server {
  host: "localhost"
  port: port
  name: "api-${env}"
}

when env == "production" {
  replicas: 3
}
"#;
        let first = format_source(source).unwrap();
        let second = format_source(&first).unwrap();
        assert_eq!(first, second, "formatting should be idempotent");
    }

    #[test]
    fn test_format_schema() {
        let source = "schema Server { host: string\nport: int(1, 65535)\ndebug?: bool }";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("schema Server {"));
        assert!(formatted.contains("  host: string"));
        assert!(formatted.contains("  port: int(1, 65535)"));
        assert!(formatted.contains("  debug?: bool"));
        assert!(formatted.contains("}"));
    }

    #[test]
    fn test_format_type_alias() {
        let source = "type Port=int(1,65535)\n\nport:8080";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("type Port = int(1, 65535)"));
    }

    #[test]
    fn test_format_import() {
        let source = "import \"./config.hone\" as config\n\nname: config.name";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("import \"./config.hone\" as config"));
    }

    #[test]
    fn test_format_named_import() {
        let source = "import { port, host } from \"./config.hone\"\n\nval: port";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("import { port, host } from \"./config.hone\""));
    }

    #[test]
    fn test_format_when() {
        let source = "let env=\"prod\"\nwhen env==\"prod\"{replicas:3}";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("when env == \"prod\" {"));
        assert!(formatted.contains("  replicas: 3"));
    }

    #[test]
    fn test_format_array_inline() {
        let source = "ports: [80, 443, 8080]";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("ports: [80, 443, 8080]"));
    }

    #[test]
    fn test_format_assertion() {
        let source = "let x = 5\nassert x > 0 : \"must be positive\"\nval: x";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("assert x > 0 : \"must be positive\""));
    }

    #[test]
    fn test_format_spread() {
        let source = "let base = { a: 1 }\nobj: { ...base, b: 2 }";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("...base"));
    }

    #[test]
    fn test_format_use_statement() {
        let source = "schema Config { name: string }\nuse Config\nname: \"test\"";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("use Config"));
    }

    #[test]
    fn test_format_for_loop() {
        let source = "items: [for x in [1, 2, 3] { x * 2 }]";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("for x in [1, 2, 3] { x * 2 }"));
    }

    #[test]
    fn test_format_ternary() {
        let source = "val: true ? 1 : 2";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("val: true ? 1 : 2"));
    }

    #[test]
    fn test_format_append_replace_operators() {
        let source = "items: [1]\nitems +: [2]\nconfig: { a: 1 }\nconfig !: { b: 2 }";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("items +: [2]"));
        assert!(formatted.contains("config !: { b: 2 }"));
    }

    #[test]
    fn test_format_empty_source() {
        let formatted = format_source("").unwrap();
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_block_comment() {
        let source = "/* This is a block comment */\nname: \"test\"";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("/* This is a block comment */"));
        assert!(formatted.contains("name: \"test\""));
    }

    #[test]
    fn test_format_preamble_body_blank_line() {
        let source = "let x = 1\nval: x";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "let x = 1\n\nval: x\n");
    }

    #[test]
    fn test_format_preamble_only() {
        let source = "let x = 42\n";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "let x = 42\n");
    }

    #[test]
    fn test_format_preamble_only_with_comments() {
        let source = "# A comment\nlet x = 42\n";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "# A comment\nlet x = 42\n");
    }

    #[test]
    fn test_format_preamble_only_multiple_lets() {
        let source = "let x = 42\nlet y = \"hello\"\n";
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, "let x = 42\nlet y = \"hello\"\n");
    }
}
