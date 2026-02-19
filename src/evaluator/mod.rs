//! Evaluator for Hone configuration language
//!
//! The evaluator takes a parsed AST and produces a Value tree.
//! It handles:
//! - Variable bindings and scoping
//! - Expression evaluation
//! - String interpolation
//! - Conditionals and loops
//! - Built-in function calls
//! - Merge semantics for assignment operators

pub mod builtins;
pub mod merge;
pub mod scope;
pub mod value;

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::errors::{HoneError, HoneResult};
use crate::lexer::token::SourceLocation;
use crate::parser::ast::*;

/// A user-defined function stored in the evaluator
#[derive(Debug, Clone)]
struct UserFunction {
    params: Vec<String>,
    body: Expr,
}

pub use merge::{merge_values, MergeBuilder, MergeStrategy};
pub use scope::{Scope, ScopeStack};
pub use value::Value;

/// Maximum expression nesting depth before the evaluator bails out
const MAX_EVAL_DEPTH: usize = 128;

/// Evaluator for Hone AST
pub struct Evaluator {
    /// Scope stack for variable bindings
    scopes: ScopeStack,
    /// Source code (for error messages)
    source: String,
    /// Whether env() and file() are allowed
    allow_env: bool,
    /// Paths marked with @unchecked annotations
    unchecked_paths: HashSet<String>,
    /// Current output key path (for tracking @unchecked)
    current_path: Vec<String>,
    /// Variant selections (variant_name -> case_name)
    variant_selections: HashMap<String, String>,
    /// User-defined functions (name -> definition)
    user_functions: HashMap<String, UserFunction>,
    /// Current recursion depth
    depth: usize,
}

impl Evaluator {
    /// Create a new evaluator
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            scopes: ScopeStack::new(),
            source: source.into(),
            allow_env: false,
            unchecked_paths: HashSet::new(),
            current_path: Vec::new(),
            variant_selections: HashMap::new(),
            user_functions: HashMap::new(),
            depth: 0,
        }
    }

    /// Set whether env() and file() are allowed
    pub fn set_allow_env(&mut self, allow: bool) {
        self.allow_env = allow;
    }

    /// Set variant selections (variant_name -> case_name)
    pub fn set_variant_selections(&mut self, selections: HashMap<String, String>) {
        self.variant_selections = selections;
    }

    /// Get paths marked with @unchecked
    pub fn unchecked_paths(&self) -> &HashSet<String> {
        &self.unchecked_paths
    }

    /// Evaluate policy declarations against the final output value.
    /// Returns a list of (policy_name, level, message) for violations.
    pub fn check_policies(
        &mut self,
        policies: &[PolicyDeclaration],
        output: &Value,
    ) -> HoneResult<Vec<(String, PolicyLevel, String)>> {
        // Inject `output` as a variable in scope
        self.scopes.define("output", output.clone());

        let mut violations = Vec::new();
        for policy in policies {
            let result = self.eval_expr(&policy.condition)?;
            let triggered = match result {
                Value::Bool(b) => b,
                _ => result.is_truthy(),
            };

            if triggered {
                let msg = policy
                    .message
                    .clone()
                    .unwrap_or_else(|| format!("policy '{}' violated", policy.name));
                violations.push((policy.name.clone(), policy.level, msg));
            }
        }
        Ok(violations)
    }

    /// Evaluate a file AST and return the result as a Value.
    ///
    /// Uses a two-pass approach over the preamble:
    /// 1. First pass evaluates let bindings, imports, expects, secrets, etc. — everything
    ///    that defines variables needed by the rest of the file.
    /// 2. Second pass evaluates variant blocks separately, because variant bodies produce
    ///    key-value output that merges into the result object (not variable definitions).
    ///    Variants must run after all let bindings are resolved so they can reference them.
    pub fn evaluate(&mut self, file: &File) -> HoneResult<Value> {
        // Pass 1: evaluate preamble items (let bindings, imports, etc.)
        for item in &file.preamble {
            self.eval_preamble_item(item)?;
        }

        // Then evaluate body items into an object
        let mut result = IndexMap::new();

        // Pass 2: evaluate variant selections and merge their body items
        for item in &file.preamble {
            if let PreambleItem::Variant(v) = item {
                self.eval_variant(v, &mut result)?;
            }
        }

        for item in &file.body {
            self.eval_body_item(item, &mut result)?;
        }

        Ok(Value::Object(result))
    }

    /// Evaluate multiple documents and return them as a vector
    pub fn evaluate_multi(&mut self, file: &File) -> HoneResult<Vec<(Option<String>, Value)>> {
        let mut results = Vec::new();

        // Evaluate main document
        let main = self.evaluate(file)?;
        results.push((None, main));

        // Evaluate sub-documents
        for doc in &file.documents {
            // Create a child scope for each document
            self.scopes.push();

            // Evaluate document preamble
            for item in &doc.preamble {
                self.eval_preamble_item(item)?;
            }

            // Evaluate document body
            let mut obj = IndexMap::new();

            // Process variant blocks from sub-document preamble
            for item in &doc.preamble {
                if let PreambleItem::Variant(variant) = item {
                    self.eval_variant(variant, &mut obj)?;
                }
            }

            for item in &doc.body {
                self.eval_body_item(item, &mut obj)?;
            }

            self.scopes.pop();

            results.push((doc.name.clone(), Value::Object(obj)));
        }

        Ok(results)
    }

    /// Evaluate a preamble item
    fn eval_preamble_item(&mut self, item: &PreambleItem) -> HoneResult<()> {
        match item {
            PreambleItem::Let(binding) => {
                let value = self.eval_expr(&binding.value)?;
                self.scopes.define(&binding.name, value);
            }
            PreambleItem::From(_) => {
                // From is handled by the merge engine, not here
                // The resolver already tracked the dependency
            }
            PreambleItem::Import(_) => {
                // Import is handled by the import resolver
                // Values would be injected into scope before evaluation
            }
            PreambleItem::Schema(_) => {
                // Schema definitions are handled by the type checker
            }
            PreambleItem::TypeAlias(_) => {
                // Type aliases are handled by the type checker
            }
            PreambleItem::Use(_) => {
                // Use statements are handled by the type checker
            }
            PreambleItem::Variant(_) => {
                // Variants are handled separately via eval_variant
            }
            PreambleItem::Expect(expect) => {
                self.eval_expect(expect)?;
            }
            PreambleItem::Secret(secret) => {
                self.eval_secret(secret)?;
            }
            PreambleItem::Policy(_) => {
                // Policies are evaluated post-compilation by the compiler
            }
            PreambleItem::FnDef(fn_def) => {
                self.user_functions.insert(
                    fn_def.name.clone(),
                    UserFunction {
                        params: fn_def.params.clone(),
                        body: fn_def.body.clone(),
                    },
                );
            }
        }
        Ok(())
    }

    /// Evaluate an expect declaration: validate args or apply defaults
    fn eval_expect(&mut self, expect: &ExpectDeclaration) -> HoneResult<()> {
        // The path must start with "args"
        if expect.path.is_empty() || expect.path[0] != "args" {
            return Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (expect.location.offset, expect.location.length).into(),
                expected: "args.name".to_string(),
                found: expect.path.join("."),
                help: "expect declarations must start with 'args', e.g.: expect args.env: string"
                    .to_string(),
            });
        }

        let arg_path = &expect.path[1..]; // skip "args"
        if arg_path.is_empty() {
            return Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (expect.location.offset, expect.location.length).into(),
                expected: "args.name".to_string(),
                found: "args".to_string(),
                help: "expect needs a field name after 'args', e.g.: expect args.env: string"
                    .to_string(),
            });
        }

        // Look up the value in args
        let current_value = self.scopes.get("args").and_then(|args| {
            let mut v = args.clone();
            for part in arg_path {
                match v {
                    Value::Object(ref obj) => {
                        v = obj.get(part)?.clone();
                    }
                    _ => return None,
                }
            }
            Some(v)
        });

        match current_value {
            Some(value) => {
                // Value exists - validate type
                let type_ok = match expect.type_name.as_str() {
                    "string" => matches!(value, Value::String(_)),
                    "int" => matches!(value, Value::Int(_)),
                    "float" => matches!(value, Value::Float(_) | Value::Int(_)),
                    "bool" => matches!(value, Value::Bool(_)),
                    "any" => true,
                    _ => true, // unknown types pass through
                };
                if !type_ok {
                    return Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (expect.location.offset, expect.location.length).into(),
                        expected: expect.type_name.clone(),
                        found: format!("{} (value: {})", value.type_name(), value),
                        help: format!(
                            "pass the correct type: --set {}=<{}>",
                            arg_path.join("."),
                            expect.type_name
                        ),
                    });
                }
            }
            None => {
                // Value not provided
                if let Some(ref default_expr) = expect.default {
                    // Apply default: inject into args object
                    let default_value = self.eval_expr(default_expr)?;
                    let mut args = self
                        .scopes
                        .get("args")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(IndexMap::new()));
                    // Navigate/create the nested path and set the value
                    {
                        let mut current = &mut args;
                        for (i, part) in arg_path.iter().enumerate() {
                            if i == arg_path.len() - 1 {
                                if let Value::Object(ref mut obj) = current {
                                    obj.insert(part.clone(), default_value.clone());
                                }
                            } else if let Value::Object(ref mut obj) = current {
                                if !obj.contains_key(part.as_str()) {
                                    obj.insert(part.clone(), Value::Object(IndexMap::new()));
                                }
                                current = obj.get_mut(part.as_str()).unwrap();
                            }
                        }
                    }
                    self.scopes.define("args", args);
                } else {
                    // No default, no value: error
                    let path_str = expect.path.join(".");
                    let arg_name = arg_path.join(".");
                    return Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (expect.location.offset, expect.location.length).into(),
                        expected: format!("{}: {}", path_str, expect.type_name),
                        found: "not provided".to_string(),
                        help: format!("provide a value: --set {}=<{}>", arg_name, expect.type_name),
                    });
                }
            }
        }
        Ok(())
    }

    /// Evaluate a secret declaration: define as placeholder string
    fn eval_secret(&mut self, secret: &SecretDeclaration) -> HoneResult<()> {
        let placeholder = format!("<SECRET:{}>", secret.provider);
        self.scopes.define(&secret.name, Value::String(placeholder));
        Ok(())
    }

    /// Evaluate a variant definition, selecting the appropriate case
    fn eval_variant(
        &mut self,
        variant: &VariantDefinition,
        target: &mut IndexMap<String, Value>,
    ) -> HoneResult<()> {
        // Find the selected case
        let selected_name = self.variant_selections.get(&variant.name).cloned();

        let case = match selected_name {
            Some(ref name) => variant
                .cases
                .iter()
                .find(|c| &c.name == name)
                .ok_or_else(|| {
                    let valid: Vec<_> = variant.cases.iter().map(|c| c.name.as_str()).collect();
                    HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (variant.location.offset, variant.location.length).into(),
                        expected: format!("one of: {}", valid.join(", ")),
                        found: name.clone(),
                        help: format!(
                            "valid cases for variant '{}': {}",
                            variant.name,
                            valid.join(", ")
                        ),
                    }
                })?,
            None => {
                // Try to find a default case
                variant.cases.iter().find(|c| c.is_default).ok_or_else(|| {
                    let valid: Vec<_> = variant.cases.iter().map(|c| c.name.as_str()).collect();
                    HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (variant.location.offset, variant.location.length).into(),
                        expected: format!("--variant {}=<case>", variant.name),
                        found: "no selection".to_string(),
                        help: format!(
                            "variant '{}' has no default case. specify with: --variant {}={}",
                            variant.name,
                            variant.name,
                            valid.join("|")
                        ),
                    }
                })?
            }
        };

        // Evaluate the selected case's body items
        for item in &case.body {
            self.eval_body_item(item, target)?;
        }

        Ok(())
    }

    /// Evaluate a body item into an object
    fn eval_body_item(
        &mut self,
        item: &BodyItem,
        target: &mut IndexMap<String, Value>,
    ) -> HoneResult<()> {
        match item {
            BodyItem::KeyValue(kv) => {
                let key = self.eval_key(&kv.key)?;
                self.current_path.push(key.clone());
                let value = self.eval_expr(&kv.value)?;
                self.current_path.pop();

                // Determine merge strategy from assignment operator
                let strategy = match kv.op {
                    AssignOp::Colon => MergeStrategy::Normal,
                    AssignOp::Append => MergeStrategy::Append,
                    AssignOp::Replace => MergeStrategy::Replace,
                };

                // Apply merge strategy
                match target.get(&key).cloned() {
                    Some(existing) => {
                        // Validate append operator usage
                        if matches!(kv.op, AssignOp::Append)
                            && !matches!(
                                (&existing, &value),
                                (Value::Array(_), Value::Array(_))
                                    | (Value::Object(_), Value::Object(_))
                            )
                        {
                            return Err(HoneError::TypeMismatch {
                                src: self.source.clone(),
                                span: (kv.location.offset, kv.location.length).into(),
                                expected: "array or object".to_string(),
                                found: value.type_name().to_string(),
                                help: "+: (append) requires both sides to be arrays or objects"
                                    .to_string(),
                            });
                        }
                        let merged = merge_values(existing, value, strategy);
                        target.insert(key, merged);
                    }
                    None => {
                        target.insert(key, value);
                    }
                }
            }
            BodyItem::Block(block) => {
                // Block is shorthand for key: { ... }
                self.current_path.push(block.name.clone());
                self.scopes.push();
                let mut obj = IndexMap::new();
                for item in &block.items {
                    self.eval_body_item(item, &mut obj)?;
                }
                self.scopes.pop();
                self.current_path.pop();

                // Merge with existing value if present (deep merge)
                let new_value = Value::Object(obj);
                match target.get(&block.name).cloned() {
                    Some(existing) => {
                        let merged = merge_values(existing, new_value, MergeStrategy::Normal);
                        target.insert(block.name.clone(), merged);
                    }
                    None => {
                        target.insert(block.name.clone(), new_value);
                    }
                }
            }
            BodyItem::When(when) => {
                self.eval_when_body(when, target)?;
            }
            BodyItem::For(for_loop) => {
                // For loops at body level merge key-value pairs into the target object
                let results = self.eval_for_in_array(for_loop)?;
                for result in results {
                    if let Value::Object(obj) = result {
                        for (k, v) in obj {
                            target.insert(k, v);
                        }
                    }
                }
            }
            BodyItem::Assert(assert) => {
                let condition = self.eval_expr(&assert.condition)?;
                if !condition.is_truthy() {
                    let message = if let Some(ref msg_expr) = assert.message {
                        let msg = self.eval_expr(msg_expr)?;
                        msg.as_str().unwrap_or("assertion failed").to_string()
                    } else {
                        "assertion failed".to_string()
                    };

                    // Build help text with variable values
                    let condition_display = assert.condition.display();
                    let variables = assert.condition.collect_variables();
                    let mut help_parts = Vec::new();
                    for var_name in &variables {
                        // Try to resolve the variable value for context
                        if let Some(val) = self.try_resolve_display_value(var_name) {
                            help_parts.push(format!("{} = {}", var_name, val));
                        }
                    }
                    let help = if help_parts.is_empty() {
                        format!("condition evaluated to false: {}", condition_display)
                    } else {
                        format!("where {}", help_parts.join(", "))
                    };

                    return Err(HoneError::AssertionFailed {
                        src: self.source.clone(),
                        span: (assert.location.offset, assert.location.length).into(),
                        condition: condition_display,
                        message,
                        help,
                    });
                }
            }
            BodyItem::Let(binding) => {
                let value = self.eval_expr(&binding.value)?;
                self.scopes.define(&binding.name, value);
            }
            BodyItem::Spread(spread) => {
                let value = self.eval_expr(&spread.expr)?;
                if let Value::Object(obj) = value {
                    for (k, v) in obj {
                        target.insert(k, v);
                    }
                } else {
                    return Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (spread.location.offset, spread.location.length).into(),
                        expected: "object".to_string(),
                        found: value.type_name().to_string(),
                        help: "spread (...) in object context requires an object".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Evaluate a key
    fn eval_key(&mut self, key: &Key) -> HoneResult<String> {
        match key {
            Key::Ident(name) => Ok(name.clone()),
            Key::String(s) => Ok(s.clone()),
            Key::Computed(expr) => {
                let value = self.eval_expr(expr)?;
                match value {
                    Value::String(s) => Ok(s),
                    Value::Int(n) => Ok(n.to_string()),
                    other => Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (expr.location().offset, expr.location().length).into(),
                        expected: "string or int".to_string(),
                        found: other.type_name().to_string(),
                        help: "computed keys must evaluate to string or int".to_string(),
                    }),
                }
            }
        }
    }

    /// Evaluate an expression
    pub fn eval_expr(&mut self, expr: &Expr) -> HoneResult<Value> {
        self.depth += 1;
        if self.depth > MAX_EVAL_DEPTH {
            let loc = expr.location();
            self.depth -= 1;
            return Err(HoneError::RecursionLimitExceeded {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                help: format!(
                    "expression nesting exceeds maximum depth of {}; simplify your configuration or split it into smaller files",
                    MAX_EVAL_DEPTH
                ),
            });
        }
        let result = self.eval_expr_inner(expr);
        self.depth -= 1;
        result
    }

    fn eval_expr_inner(&mut self, expr: &Expr) -> HoneResult<Value> {
        match expr {
            Expr::Null(_) => Ok(Value::Null),
            Expr::Bool(b, _) => Ok(Value::Bool(*b)),
            Expr::Integer(n, _) => Ok(Value::Int(*n)),
            Expr::Float(n, _) => Ok(Value::Float(*n)),
            Expr::String(s) => self.eval_string_expr(s),
            Expr::Ident(name, loc) => self.eval_ident(name, loc),
            Expr::Path(path) => self.eval_path(path),
            Expr::Array(arr) => self.eval_array(arr),
            Expr::Object(obj) => self.eval_object(obj),
            Expr::Binary(bin) => self.eval_binary(bin),
            Expr::Unary(unary) => self.eval_unary(unary),
            Expr::Call(call) => self.eval_call(call),
            Expr::Index(idx) => self.eval_index(idx),
            Expr::Conditional(cond) => self.eval_conditional(cond),
            Expr::Annotated(ann) => {
                // Record @unchecked paths for the type checker to skip
                if ann.constraint.name == "unchecked" {
                    let path = self.current_path.join(".");
                    if !path.is_empty() {
                        self.unchecked_paths.insert(path);
                    }
                }
                // Type annotations are checked by the type checker
                // Here we just evaluate the expression
                self.eval_expr(&ann.expr)
            }
            Expr::Paren(inner, _) => self.eval_expr(inner),
            Expr::For(for_loop) => self.eval_for_expr(for_loop),
            Expr::When(when) => self.eval_when_expr(when),
        }
    }

    /// Evaluate a string expression (with potential interpolation)
    fn eval_string_expr(&mut self, expr: &StringExpr) -> HoneResult<Value> {
        let mut result = String::new();

        for part in &expr.parts {
            match part {
                StringPart::Literal(s) => result.push_str(s),
                StringPart::Interpolation(e) => {
                    let value = self.eval_expr(e)?;
                    result.push_str(&value.to_string());
                }
            }
        }

        Ok(Value::String(result))
    }

    /// Evaluate an identifier reference
    fn eval_ident(&self, name: &str, loc: &SourceLocation) -> HoneResult<Value> {
        if let Some(value) = self.scopes.get(name) {
            Ok(value.clone())
        } else {
            let available = self.scopes.available_names();
            let help = crate::errors::undefined_variable_help(name, &available);
            Err(HoneError::undefined_variable(
                self.source.clone(),
                loc,
                name,
                help,
            ))
        }
    }

    /// Evaluate a path expression (a.b.c)
    fn eval_path(&mut self, path: &PathExpr) -> HoneResult<Value> {
        if path.parts.is_empty() {
            return Err(HoneError::unexpected_token(
                self.source.clone(),
                &path.location,
                "path",
                "empty path",
                "path expressions must have at least one part",
            ));
        }

        // Get the first part (must be an identifier)
        let first = match &path.parts[0] {
            PathPart::Ident(name) => name,
            PathPart::Index(_) => {
                return Err(HoneError::unexpected_token(
                    self.source.clone(),
                    &path.location,
                    "identifier",
                    "index",
                    "path must start with an identifier",
                ));
            }
        };

        // Look up the base value
        let mut current = self.eval_ident(first, &path.location)?;

        // Navigate through remaining parts
        for (_i, part) in path.parts.iter().enumerate().skip(1) {
            match part {
                PathPart::Ident(name) => {
                    current = match current {
                        Value::Object(ref obj) => obj.get(name).cloned().unwrap_or(Value::Null),
                        _ => {
                            return Err(HoneError::TypeMismatch {
                                src: self.source.clone(),
                                span: (path.location.offset, path.location.length).into(),
                                expected: "object".to_string(),
                                found: current.type_name().to_string(),
                                help: format!("cannot access .{} on {}", name, current.type_name()),
                            });
                        }
                    };
                }
                PathPart::Index(idx_expr) => {
                    let idx = self.eval_expr(idx_expr)?;
                    current = match (&current, &idx) {
                        (Value::Array(arr), Value::Int(i)) => {
                            let i = *i as usize;
                            if i < arr.len() {
                                arr[i].clone()
                            } else {
                                return Err(HoneError::TypeMismatch {
                                    src: self.source.clone(),
                                    span: (path.location.offset, path.location.length).into(),
                                    expected: format!("index < {}", arr.len()),
                                    found: i.to_string(),
                                    help: "array index out of bounds".to_string(),
                                });
                            }
                        }
                        (Value::Object(obj), Value::String(key)) => {
                            obj.get(key).cloned().unwrap_or(Value::Null)
                        }
                        _ => {
                            return Err(HoneError::TypeMismatch {
                                src: self.source.clone(),
                                span: (path.location.offset, path.location.length).into(),
                                expected: "array with int index or object with string key"
                                    .to_string(),
                                found: format!("{}[{}]", current.type_name(), idx.type_name()),
                                help: "invalid indexing".to_string(),
                            });
                        }
                    };
                }
            }
        }

        Ok(current)
    }

    /// Evaluate an array literal
    fn eval_array(&mut self, arr: &ArrayExpr) -> HoneResult<Value> {
        let mut result = Vec::new();

        for elem in &arr.elements {
            match elem {
                ArrayElement::Expr(e) => {
                    result.push(self.eval_expr(e)?);
                }
                ArrayElement::Spread(e) => {
                    let value = self.eval_expr(e)?;
                    if let Value::Array(items) = value {
                        result.extend(items);
                    } else {
                        return Err(HoneError::TypeMismatch {
                            src: self.source.clone(),
                            span: (e.location().offset, e.location().length).into(),
                            expected: "array".to_string(),
                            found: value.type_name().to_string(),
                            help: "spread (...) requires an array".to_string(),
                        });
                    }
                }
                ArrayElement::For(for_loop) => {
                    let items = self.eval_for_in_array(for_loop)?;
                    result.extend(items);
                }
                ArrayElement::When(when) => {
                    self.eval_when_array(when, &mut result)?;
                }
            }
        }

        Ok(Value::Array(result))
    }

    /// Evaluate an object literal
    fn eval_object(&mut self, obj: &ObjectExpr) -> HoneResult<Value> {
        self.scopes.push();
        let mut result = IndexMap::new();

        for item in &obj.items {
            self.eval_body_item(item, &mut result)?;
        }

        self.scopes.pop();
        Ok(Value::Object(result))
    }

    /// Evaluate a for loop in array context
    fn eval_for_in_array(&mut self, for_loop: &ForLoop) -> HoneResult<Vec<Value>> {
        let iterable = self.eval_expr(&for_loop.iterable)?;
        let items = match iterable {
            Value::Array(arr) => arr.into_iter().enumerate().collect(),
            Value::Object(obj) => obj
                .into_iter()
                .enumerate()
                .map(|(i, (k, v))| {
                    let mut pair = IndexMap::new();
                    pair.insert("key".to_string(), Value::String(k));
                    pair.insert("value".to_string(), v);
                    (i, Value::Object(pair))
                })
                .collect::<Vec<_>>(),
            other => {
                return Err(HoneError::TypeMismatch {
                    src: self.source.clone(),
                    span: (for_loop.location.offset, for_loop.location.length).into(),
                    expected: "array or object".to_string(),
                    found: other.type_name().to_string(),
                    help: "for loop requires an iterable".to_string(),
                });
            }
        };

        let mut result = Vec::new();

        for (idx, item) in items {
            self.scopes.push();

            // Bind loop variable(s)
            match &for_loop.binding {
                ForBinding::Single(name) => {
                    self.scopes.define(name, item);
                }
                ForBinding::Pair(k, v) => {
                    if let Value::Object(obj) = item {
                        // Object iteration: pair binding gives (key, value)
                        if let (Some(key), Some(val)) = (obj.get("key"), obj.get("value")) {
                            self.scopes.define(k, key.clone());
                            self.scopes.define(v, val.clone());
                        }
                    } else {
                        // Array iteration: pair binding gives (index, element)
                        self.scopes.define(k, Value::Int(idx as i64));
                        self.scopes.define(v, item);
                    }
                }
            }

            // Evaluate body
            match &for_loop.body {
                ForBody::Expr(e) => {
                    result.push(self.eval_expr(e)?);
                }
                ForBody::Object(items) => {
                    let mut obj = IndexMap::new();
                    for item in items {
                        self.eval_body_item(item, &mut obj)?;
                    }
                    result.push(Value::Object(obj));
                }
                ForBody::Block(items, expr) => {
                    let mut obj = IndexMap::new();
                    for item in items {
                        self.eval_body_item(item, &mut obj)?;
                    }
                    result.push(self.eval_expr(expr)?);
                }
            }

            self.scopes.pop();
        }

        Ok(result)
    }

    /// Evaluate a for expression
    fn eval_for_expr(&mut self, for_loop: &ForLoop) -> HoneResult<Value> {
        let items = self.eval_for_in_array(for_loop)?;
        Ok(Value::Array(items))
    }

    /// Evaluate a binary expression
    fn eval_binary(&mut self, bin: &BinaryExpr) -> HoneResult<Value> {
        let left = self.eval_expr(&bin.left)?;

        // Short-circuit evaluation for && and ||
        match bin.op {
            BinaryOp::And => {
                if !left.is_truthy() {
                    return Ok(Value::Bool(false));
                }
                let right = self.eval_expr(&bin.right)?;
                return Ok(Value::Bool(right.is_truthy()));
            }
            BinaryOp::Or => {
                if left.is_truthy() {
                    return Ok(Value::Bool(true));
                }
                let right = self.eval_expr(&bin.right)?;
                return Ok(Value::Bool(right.is_truthy()));
            }
            BinaryOp::NullCoalesce => {
                if !left.is_null() {
                    return Ok(left);
                }
                return self.eval_expr(&bin.right);
            }
            _ => {}
        }

        let right = self.eval_expr(&bin.right)?;

        match bin.op {
            BinaryOp::Add => self.eval_add(&left, &right, &bin.location),
            BinaryOp::Sub => self.eval_sub(&left, &right, &bin.location),
            BinaryOp::Mul => self.eval_mul(&left, &right, &bin.location),
            BinaryOp::Div => self.eval_div(&left, &right, &bin.location),
            BinaryOp::Mod => self.eval_mod(&left, &right, &bin.location),
            BinaryOp::Eq => Ok(Value::Bool(left.equals(&right))),
            BinaryOp::NotEq => Ok(Value::Bool(!left.equals(&right))),
            BinaryOp::Lt => self.eval_comparison(&left, &right, |a, b| a < b, &bin.location),
            BinaryOp::Gt => self.eval_comparison(&left, &right, |a, b| a > b, &bin.location),
            BinaryOp::LtEq => self.eval_comparison(&left, &right, |a, b| a <= b, &bin.location),
            BinaryOp::GtEq => self.eval_comparison(&left, &right, |a, b| a >= b, &bin.location),
            BinaryOp::And | BinaryOp::Or | BinaryOp::NullCoalesce => {
                unreachable!("handled above")
            }
        }
    }

    /// Shared numeric arithmetic: dispatches Int*Int (checked), Float*Float,
    /// Int*Float, and Float*Int cases. Returns None for non-numeric operands.
    fn eval_numeric(
        &self,
        left: &Value,
        right: &Value,
        loc: &SourceLocation,
        op_sym: &str,
        checked_int: fn(i64, i64) -> Option<i64>,
        float_op: fn(f64, f64) -> f64,
    ) -> HoneResult<Option<Value>> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                let result = checked_int(*a, *b).map(Value::Int).ok_or_else(|| {
                    HoneError::ArithmeticOverflow {
                        src: self.source.clone(),
                        span: (loc.offset, loc.length).into(),
                        operation: format!("{} {} {}", a, op_sym, b),
                        help: "integer overflow: result exceeds i64 range".to_string(),
                    }
                })?;
                Ok(Some(result))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Some(Value::Float(float_op(*a, *b)))),
            (Value::Int(a), Value::Float(b)) => Ok(Some(Value::Float(float_op(*a as f64, *b)))),
            (Value::Float(a), Value::Int(b)) => Ok(Some(Value::Float(float_op(*a, *b as f64)))),
            _ => Ok(None),
        }
    }

    fn eval_add(&self, left: &Value, right: &Value, loc: &SourceLocation) -> HoneResult<Value> {
        if let Some(result) =
            self.eval_numeric(left, right, loc, "+", i64::checked_add, |a, b| a + b)?
        {
            return Ok(result);
        }
        match (left, right) {
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            (Value::Array(a), Value::Array(b)) => {
                let mut result = a.clone();
                result.extend(b.clone());
                Ok(Value::Array(result))
            }
            _ => Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                expected: "numbers, strings, or arrays".to_string(),
                found: format!("{} + {}", left.type_name(), right.type_name()),
                help: "cannot add these types".to_string(),
            }),
        }
    }

    fn eval_sub(&self, left: &Value, right: &Value, loc: &SourceLocation) -> HoneResult<Value> {
        self.eval_numeric(left, right, loc, "-", i64::checked_sub, |a, b| a - b)?
            .ok_or_else(|| HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                expected: "numbers".to_string(),
                found: format!("{} - {}", left.type_name(), right.type_name()),
                help: "cannot subtract these types".to_string(),
            })
    }

    fn eval_mul(&self, left: &Value, right: &Value, loc: &SourceLocation) -> HoneResult<Value> {
        self.eval_numeric(left, right, loc, "*", i64::checked_mul, |a, b| a * b)?
            .ok_or_else(|| HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                expected: "numbers".to_string(),
                found: format!("{} * {}", left.type_name(), right.type_name()),
                help: "cannot multiply these types".to_string(),
            })
    }

    fn eval_div(&self, left: &Value, right: &Value, loc: &SourceLocation) -> HoneResult<Value> {
        // Check for division by zero
        let is_zero = match right {
            Value::Int(0) => true,
            Value::Float(n) if *n == 0.0 => true,
            _ => false,
        };
        if is_zero {
            return Err(HoneError::DivisionByZero {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
            });
        }

        self.eval_numeric(left, right, loc, "/", i64::checked_div, |a, b| a / b)?
            .ok_or_else(|| HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                expected: "numbers".to_string(),
                found: format!("{} / {}", left.type_name(), right.type_name()),
                help: "cannot divide these types".to_string(),
            })
    }

    fn eval_mod(&self, left: &Value, right: &Value, loc: &SourceLocation) -> HoneResult<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    Err(HoneError::DivisionByZero {
                        src: self.source.clone(),
                        span: (loc.offset, loc.length).into(),
                    })
                } else {
                    a.checked_rem(*b)
                        .map(Value::Int)
                        .ok_or_else(|| HoneError::ArithmeticOverflow {
                            src: self.source.clone(),
                            span: (loc.offset, loc.length).into(),
                            operation: format!("{} % {}", a, b),
                            help: "integer overflow: i64::MIN % -1 overflows".to_string(),
                        })
                }
            }
            _ => Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (loc.offset, loc.length).into(),
                expected: "integers".to_string(),
                found: format!("{} % {}", left.type_name(), right.type_name()),
                help: "modulo requires integers".to_string(),
            }),
        }
    }

    fn eval_comparison<F>(
        &self,
        left: &Value,
        right: &Value,
        op: F,
        loc: &SourceLocation,
    ) -> HoneResult<Value>
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left.to_number(), right.to_number()) {
            (Some(a), Some(b)) => Ok(Value::Bool(op(a, b))),
            _ => {
                // String comparison: convert Ordering to i32 (-1/0/1) then to f64
                // so we can reuse the same `op` closure (e.g. PartialOrd::lt on f64).
                // Comparing the ordering-as-number against 0.0 maps < => -1<0, > => 1>0, etc.
                match (left, right) {
                    (Value::String(a), Value::String(b)) => {
                        Ok(Value::Bool(op(a.cmp(b) as i32 as f64, 0.0)))
                    }
                    _ => Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (loc.offset, loc.length).into(),
                        expected: "comparable values".to_string(),
                        found: format!("{} vs {}", left.type_name(), right.type_name()),
                        help: "cannot compare these types".to_string(),
                    }),
                }
            }
        }
    }

    /// Evaluate a unary expression
    fn eval_unary(&mut self, unary: &UnaryExpr) -> HoneResult<Value> {
        let operand = self.eval_expr(&unary.operand)?;

        match unary.op {
            UnaryOp::Not => Ok(Value::Bool(!operand.is_truthy())),
            UnaryOp::Neg => {
                match operand {
                    Value::Int(n) => n.checked_neg().map(Value::Int).ok_or_else(|| {
                        HoneError::ArithmeticOverflow {
                            src: self.source.clone(),
                            span: (unary.location.offset, unary.location.length).into(),
                            operation: format!("-{}", n),
                            help: "integer overflow: negating i64::MIN overflows".to_string(),
                        }
                    }),
                    Value::Float(n) => Ok(Value::Float(-n)),
                    _ => Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (unary.location.offset, unary.location.length).into(),
                        expected: "number".to_string(),
                        found: operand.type_name().to_string(),
                        help: "unary minus requires a number".to_string(),
                    }),
                }
            }
        }
    }

    /// Evaluate a function call
    fn eval_call(&mut self, call: &CallExpr) -> HoneResult<Value> {
        // Get the function name
        let func_name = match &*call.func {
            Expr::Ident(name, _) => name.clone(),
            Expr::Path(path) => {
                // For now, only support simple function names
                // Method calls (obj.method()) would need more work
                if path.parts.len() == 1 {
                    if let PathPart::Ident(name) = &path.parts[0] {
                        name.clone()
                    } else {
                        return Err(HoneError::TypeMismatch {
                            src: self.source.clone(),
                            span: (call.location.offset, call.location.length).into(),
                            expected: "function name".to_string(),
                            found: "path expression".to_string(),
                            help: "only simple function names are supported".to_string(),
                        });
                    }
                } else {
                    return Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (call.location.offset, call.location.length).into(),
                        expected: "function name".to_string(),
                        found: "path expression".to_string(),
                        help: "method calls are not supported".to_string(),
                    });
                }
            }
            _ => {
                return Err(HoneError::TypeMismatch {
                    src: self.source.clone(),
                    span: (call.location.offset, call.location.length).into(),
                    expected: "function name".to_string(),
                    found: "expression".to_string(),
                    help: "only named function calls are supported".to_string(),
                });
            }
        };

        // Evaluate arguments
        let args: Vec<Value> = call
            .args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<HoneResult<_>>()?;

        // Check user-defined functions first
        if let Some(user_fn) = self.user_functions.get(&func_name).cloned() {
            if args.len() != user_fn.params.len() {
                return Err(HoneError::TypeMismatch {
                    src: self.source.clone(),
                    span: (call.location.offset, call.location.length).into(),
                    expected: format!(
                        "{} argument(s) for fn {}",
                        user_fn.params.len(),
                        func_name
                    ),
                    found: format!("{} argument(s)", args.len()),
                    help: format!(
                        "fn {}({}) takes exactly {} argument(s)",
                        func_name,
                        user_fn.params.join(", "),
                        user_fn.params.len()
                    ),
                });
            }

            // Create a new scope with parameter bindings
            self.scopes.push();
            for (param, arg) in user_fn.params.iter().zip(args.iter()) {
                self.scopes.define(param, arg.clone());
            }

            let result = self.eval_expr(&user_fn.body);
            self.scopes.pop();
            return result;
        }

        // Gate env/file behind --allow-env
        if !self.allow_env && (func_name == "env" || func_name == "file") {
            let help = if func_name == "env" {
                "env() reads environment variables, making output non-deterministic\n  = in CI/CD, prefer: --set key=\"$VALUE\"\n  = for local development: hone compile --allow-env <file>".to_string()
            } else {
                "file() reads external files, making output non-deterministic\n  = for local development: hone compile --allow-env <file>".to_string()
            };
            return Err(HoneError::EnvNotAllowed {
                src: self.source.clone(),
                span: (call.location.offset, call.location.length).into(),
                func_name: func_name.clone(),
                help,
            });
        }

        // Call built-in function
        builtins::call_builtin(&func_name, args, &call.location, &self.source)
    }

    /// Evaluate an index expression
    fn eval_index(&mut self, idx: &IndexExpr) -> HoneResult<Value> {
        let base = self.eval_expr(&idx.base)?;
        let index = self.eval_expr(&idx.index)?;

        match (&base, &index) {
            (Value::Array(arr), Value::Int(i)) => {
                let i = *i as usize;
                if i < arr.len() {
                    Ok(arr[i].clone())
                } else {
                    Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (idx.location.offset, idx.location.length).into(),
                        expected: format!("index < {}", arr.len()),
                        found: i.to_string(),
                        help: "array index out of bounds".to_string(),
                    })
                }
            }
            (Value::Object(obj), Value::String(key)) => {
                Ok(obj.get(key).cloned().unwrap_or(Value::Null))
            }
            _ => Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (idx.location.offset, idx.location.length).into(),
                expected: "array[int] or object[string]".to_string(),
                found: format!("{}[{}]", base.type_name(), index.type_name()),
                help: "invalid indexing".to_string(),
            }),
        }
    }

    /// Evaluate a conditional expression
    fn eval_conditional(&mut self, cond: &ConditionalExpr) -> HoneResult<Value> {
        let condition = self.eval_expr(&cond.condition)?;

        if condition.is_truthy() {
            self.eval_expr(&cond.then_branch)
        } else {
            self.eval_expr(&cond.else_branch)
        }
    }

    /// Define a variable in the current scope (for external use)
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.scopes.define(name, value);
    }

    /// Add an import to the current scope
    pub fn add_import(&mut self, alias: impl Into<String>, value: Value) {
        self.scopes.add_import(alias, value);
    }

    /// Lookup a variable in the current scope
    pub fn lookup(&self, name: &str) -> Option<Value> {
        self.scopes.get(name).cloned()
    }

    /// Get user-defined function names (for export)
    pub fn user_function_names(&self) -> Vec<String> {
        self.user_functions.keys().cloned().collect()
    }

    /// Register a user-defined function (for import)
    pub fn register_user_function(&mut self, name: String, params: Vec<String>, body: Expr) {
        self.user_functions
            .insert(name, UserFunction { params, body });
    }

    /// Evaluate a when/else chain in body context (merges items into target object)
    fn eval_when_body(
        &mut self,
        when: &WhenBlock,
        target: &mut IndexMap<String, Value>,
    ) -> HoneResult<()> {
        let condition = self.eval_expr(&when.condition)?;
        if condition.is_truthy() {
            for item in &when.body {
                self.eval_body_item(item, target)?;
            }
        } else if let Some(ref else_branch) = when.else_branch {
            match else_branch {
                ElseBranch::ElseWhen(else_when) => {
                    self.eval_when_body(else_when, target)?;
                }
                ElseBranch::Else(else_body, _) => {
                    for item in else_body {
                        self.eval_body_item(item, target)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Evaluate a when/else chain in expression context (returns a Value)
    fn eval_when_expr(&mut self, when: &WhenBlock) -> HoneResult<Value> {
        let condition = self.eval_expr(&when.condition)?;
        if condition.is_truthy() {
            let mut obj = IndexMap::new();
            for item in &when.body {
                self.eval_body_item(item, &mut obj)?;
            }
            Ok(Value::Object(obj))
        } else if let Some(ref else_branch) = when.else_branch {
            match else_branch {
                ElseBranch::ElseWhen(else_when) => self.eval_when_expr(else_when),
                ElseBranch::Else(else_body, _) => {
                    let mut obj = IndexMap::new();
                    for item in else_body {
                        self.eval_body_item(item, &mut obj)?;
                    }
                    Ok(Value::Object(obj))
                }
            }
        } else {
            Ok(Value::Null)
        }
    }

    /// Evaluate a when/else chain in array context (pushes elements into result)
    fn eval_when_array(&mut self, when: &WhenBlock, result: &mut Vec<Value>) -> HoneResult<()> {
        let condition = self.eval_expr(&when.condition)?;
        if condition.is_truthy() {
            for item in &when.body {
                if let BodyItem::KeyValue(kv) = item {
                    result.push(self.eval_expr(&kv.value)?);
                }
            }
        } else if let Some(ref else_branch) = when.else_branch {
            match else_branch {
                ElseBranch::ElseWhen(else_when) => {
                    self.eval_when_array(else_when, result)?;
                }
                ElseBranch::Else(else_body, _) => {
                    for item in else_body {
                        if let BodyItem::KeyValue(kv) = item {
                            result.push(self.eval_expr(&kv.value)?);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Try to resolve a variable name (possibly dotted) to a display string.
    /// Used for assertion error context. Returns None if not resolvable.
    fn try_resolve_display_value(&self, name: &str) -> Option<String> {
        let parts: Vec<&str> = name.split('.').collect();
        let root = self.scopes.get(parts[0])?;

        let mut value = root.clone();
        for part in &parts[1..] {
            match value {
                Value::Object(ref obj) => {
                    value = obj.get(*part)?.clone();
                }
                _ => return None,
            }
        }

        Some(match &value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => format!("\"{}\"", s),
            Value::Array(arr) => format!("[...] (length {})", arr.len()),
            Value::Object(obj) => format!("{{...}} ({} keys)", obj.len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn eval(source: &str) -> HoneResult<Value> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = Evaluator::new(source);
        evaluator.evaluate(&ast)
    }

    #[test]
    fn test_simple_key_value() {
        let result = eval("name: \"hello\"").unwrap();
        assert_eq!(
            result.get_path(&["name"]),
            Some(&Value::String("hello".into()))
        );
    }

    #[test]
    fn test_let_binding() {
        let result = eval("let x = 42\nvalue: x").unwrap();
        assert_eq!(result.get_path(&["value"]), Some(&Value::Int(42)));
    }

    #[test]
    fn test_arithmetic() {
        let result = eval("x: 1 + 2 * 3").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Int(7)));
    }

    #[test]
    fn test_string_interpolation() {
        let result = eval("let name = \"world\"\nmsg: \"hello ${name}\"").unwrap();
        assert_eq!(
            result.get_path(&["msg"]),
            Some(&Value::String("hello world".into()))
        );
    }

    #[test]
    fn test_conditional_expr() {
        let result = eval("let x = true\nval: x ? 1 : 2").unwrap();
        assert_eq!(result.get_path(&["val"]), Some(&Value::Int(1)));
    }

    #[test]
    fn test_when_block() {
        let result = eval("let env = \"prod\"\nwhen env == \"prod\" { replicas: 3 }").unwrap();
        assert_eq!(result.get_path(&["replicas"]), Some(&Value::Int(3)));
    }

    #[test]
    fn test_when_block_false() {
        let result = eval("let env = \"dev\"\nwhen env == \"prod\" { replicas: 3 }").unwrap();
        assert_eq!(result.get_path(&["replicas"]), None);
    }

    #[test]
    fn test_array_literal() {
        let result = eval("arr: [1, 2, 3]").unwrap();
        assert_eq!(
            result.get_path(&["arr"]),
            Some(&Value::Array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))
        );
    }

    #[test]
    fn test_object_literal() {
        let result = eval("obj: { a: 1, b: 2 }").unwrap();
        if let Some(Value::Object(obj)) = result.get_path(&["obj"]) {
            assert_eq!(obj.get("a"), Some(&Value::Int(1)));
            assert_eq!(obj.get("b"), Some(&Value::Int(2)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_for_in_array() {
        let result = eval("items: [for x in [1, 2, 3] { x * 2 }]").unwrap();
        assert_eq!(
            result.get_path(&["items"]),
            Some(&Value::Array(vec![
                Value::Int(2),
                Value::Int(4),
                Value::Int(6)
            ]))
        );
    }

    #[test]
    fn test_for_with_object_body() {
        let result = eval("items: [for i in [1, 2] { name: \"item-${i}\" }]").unwrap();
        if let Some(Value::Array(arr)) = result.get_path(&["items"]) {
            assert_eq!(arr.len(), 2);
            if let Value::Object(obj) = &arr[0] {
                assert_eq!(obj.get("name"), Some(&Value::String("item-1".into())));
            }
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_block_syntax() {
        let result = eval("server {\n  host: \"localhost\"\n  port: 8080\n}").unwrap();
        if let Some(Value::Object(server)) = result.get_path(&["server"]) {
            assert_eq!(server.get("host"), Some(&Value::String("localhost".into())));
            assert_eq!(server.get("port"), Some(&Value::Int(8080)));
        } else {
            panic!("expected server object");
        }
    }

    #[test]
    fn test_nested_blocks() {
        let result = eval("server { config { debug: true } }").unwrap();
        assert_eq!(
            result.get_path(&["server", "config", "debug"]),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn test_path_expression() {
        let result = eval("let cfg = { port: 8080 }\nport: cfg.port").unwrap();
        assert_eq!(result.get_path(&["port"]), Some(&Value::Int(8080)));
    }

    #[test]
    fn test_index_expression() {
        let result = eval("let arr = [1, 2, 3]\nval: arr[1]").unwrap();
        assert_eq!(result.get_path(&["val"]), Some(&Value::Int(2)));
    }

    #[test]
    fn test_builtin_len() {
        let result = eval("x: len([1, 2, 3])").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Int(3)));
    }

    #[test]
    fn test_builtin_range() {
        let result = eval("x: range(3)").unwrap();
        assert_eq!(
            result.get_path(&["x"]),
            Some(&Value::Array(vec![
                Value::Int(0),
                Value::Int(1),
                Value::Int(2)
            ]))
        );
    }

    #[test]
    fn test_unary_neg() {
        let result = eval("x: -5").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Int(-5)));
    }

    #[test]
    fn test_unary_not() {
        let result = eval("x: !true").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_comparison() {
        let result = eval("x: 5 > 3").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_logical_and() {
        let result = eval("x: true && false").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_logical_or() {
        let result = eval("x: true || false").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_null_coalesce() {
        let result = eval("x: null ?? 42").unwrap();
        assert_eq!(result.get_path(&["x"]), Some(&Value::Int(42)));
    }

    #[test]
    fn test_spread_in_array() {
        let result = eval("let a = [1, 2]\narr: [...a, 3]").unwrap();
        assert_eq!(
            result.get_path(&["arr"]),
            Some(&Value::Array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))
        );
    }

    #[test]
    fn test_spread_in_object() {
        let result = eval("let base = { a: 1 }\nobj: { ...base, b: 2 }").unwrap();
        if let Some(Value::Object(obj)) = result.get_path(&["obj"]) {
            assert_eq!(obj.get("a"), Some(&Value::Int(1)));
            assert_eq!(obj.get("b"), Some(&Value::Int(2)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_undefined_variable_error() {
        let result = eval("x: undefined_var");
        assert!(result.is_err());
    }

    #[test]
    fn test_type_error_on_add() {
        let result = eval("x: 1 + \"hello\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_pass() {
        let result = eval("let x = 5\nassert x > 0\nval: x");
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_fail() {
        let result = eval("let x = 0\nassert x > 0: \"x must be positive\"");
        assert!(result.is_err());
        if let Err(HoneError::AssertionFailed { message, .. }) = result {
            assert_eq!(message, "x must be positive");
        }
    }

    #[test]
    fn test_complete_example() {
        let source = r#"
let env = "production"
let base_port = 8000

server {
  host: "localhost"
  port: base_port + 1
  name: "api-${env}"
  ssl: true
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
"#;
        let result = eval(source).unwrap();

        assert_eq!(
            result.get_path(&["server", "host"]),
            Some(&Value::String("localhost".into()))
        );
        assert_eq!(
            result.get_path(&["server", "port"]),
            Some(&Value::Int(8001))
        );
        assert_eq!(
            result.get_path(&["server", "name"]),
            Some(&Value::String("api-production".into()))
        );
        assert_eq!(result.get_path(&["replicas"]), Some(&Value::Int(3)));

        if let Some(Value::Array(containers)) = result.get_path(&["containers"]) {
            assert_eq!(containers.len(), 3);
        } else {
            panic!("expected containers array");
        }
    }

    #[test]
    fn test_deep_merge_objects() {
        // Test that objects with same key get deep merged
        let source = r#"
server {
  host: "localhost"
  port: 8080
}
server {
  port: 9000
  debug: true
}
"#;
        let result = eval(source).unwrap();
        if let Some(Value::Object(server)) = result.get_path(&["server"]) {
            assert_eq!(server.get("host"), Some(&Value::String("localhost".into())));
            assert_eq!(server.get("port"), Some(&Value::Int(9000))); // Second wins
            assert_eq!(server.get("debug"), Some(&Value::Bool(true)));
        } else {
            panic!("expected server object");
        }
    }

    #[test]
    fn test_append_operator_arrays() {
        let source = r#"
items: [1, 2]
items +: [3, 4]
"#;
        let result = eval(source).unwrap();
        if let Some(Value::Array(items)) = result.get_path(&["items"]) {
            assert_eq!(items.len(), 4);
            assert_eq!(items[0], Value::Int(1));
            assert_eq!(items[1], Value::Int(2));
            assert_eq!(items[2], Value::Int(3));
            assert_eq!(items[3], Value::Int(4));
        } else {
            panic!("expected items array");
        }
    }

    #[test]
    fn test_append_operator_objects() {
        let source = r#"
config: { a: 1 }
config +: { b: 2 }
"#;
        let result = eval(source).unwrap();
        if let Some(Value::Object(config)) = result.get_path(&["config"]) {
            assert_eq!(config.get("a"), Some(&Value::Int(1)));
            assert_eq!(config.get("b"), Some(&Value::Int(2)));
        } else {
            panic!("expected config object");
        }
    }

    #[test]
    fn test_replace_operator() {
        let source = r#"
server: { host: "old", port: 8080 }
server !: { name: "new" }
"#;
        let result = eval(source).unwrap();
        if let Some(Value::Object(server)) = result.get_path(&["server"]) {
            // Replace completely ignores previous value
            assert_eq!(server.get("host"), None);
            assert_eq!(server.get("port"), None);
            assert_eq!(server.get("name"), Some(&Value::String("new".into())));
        } else {
            panic!("expected server object");
        }
    }

    #[test]
    fn test_nested_deep_merge() {
        let source = r#"
config {
  server {
    host: "localhost"
  }
}
config {
  server {
    port: 8080
  }
  client {
    timeout: 30
  }
}
"#;
        let result = eval(source).unwrap();
        assert_eq!(
            result.get_path(&["config", "server", "host"]),
            Some(&Value::String("localhost".into()))
        );
        assert_eq!(
            result.get_path(&["config", "server", "port"]),
            Some(&Value::Int(8080))
        );
        assert_eq!(
            result.get_path(&["config", "client", "timeout"]),
            Some(&Value::Int(30))
        );
    }

    #[test]
    fn test_append_type_mismatch_error() {
        // Append requires compatible types
        let result = eval("x: 1\nx +: [2]");
        assert!(result.is_err());
    }
}
