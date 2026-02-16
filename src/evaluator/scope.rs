//! Variable scoping for the Hone evaluator
//!
//! Scopes are lexically nested and support:
//! - Let bindings
//! - Import aliases
//! - Schema definitions

use std::collections::HashMap;

use super::value::Value;

/// A scope containing variable bindings
#[derive(Debug, Clone)]
pub struct Scope {
    /// Variable bindings in this scope
    bindings: HashMap<String, Value>,
    /// Parent scope (for lexical nesting)
    parent: Option<Box<Scope>>,
    /// Imported modules (alias -> module values)
    imports: HashMap<String, Value>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    /// Create a new empty scope
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            parent: None,
            imports: HashMap::new(),
        }
    }

    /// Create a new scope with a parent
    pub fn with_parent(parent: Scope) -> Self {
        Self {
            bindings: HashMap::new(),
            parent: Some(Box::new(parent)),
            imports: HashMap::new(),
        }
    }

    /// Create a child scope (borrows from parent)
    pub fn child(&self) -> Self {
        Self {
            bindings: HashMap::new(),
            parent: Some(Box::new(self.clone())),
            imports: HashMap::new(),
        }
    }

    /// Define a variable in this scope
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
    }

    /// Look up a variable by name (searches parent scopes)
    pub fn get(&self, name: &str) -> Option<&Value> {
        if let Some(value) = self.bindings.get(name) {
            return Some(value);
        }

        // Check imports
        if let Some(value) = self.imports.get(name) {
            return Some(value);
        }

        // Check parent scope
        if let Some(ref parent) = self.parent {
            return parent.get(name);
        }

        None
    }

    /// Check if a variable is defined anywhere in scope chain
    pub fn is_defined(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Add an import alias
    pub fn add_import(&mut self, alias: impl Into<String>, value: Value) {
        self.imports.insert(alias.into(), value);
    }

    /// Get an imported module by alias
    pub fn get_import(&self, alias: &str) -> Option<&Value> {
        if let Some(value) = self.imports.get(alias) {
            return Some(value);
        }

        if let Some(ref parent) = self.parent {
            return parent.get_import(alias);
        }

        None
    }
}

/// A scope stack for evaluation contexts
#[derive(Debug, Default)]
pub struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Create a new scope stack with a global scope
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::new()],
        }
    }

    /// Push a new scope onto the stack
    pub fn push(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop a scope from the stack
    pub fn pop(&mut self) -> Option<Scope> {
        if self.scopes.len() > 1 {
            self.scopes.pop()
        } else {
            None // Don't pop the global scope
        }
    }

    /// Get the current (topmost) scope
    pub fn current(&self) -> &Scope {
        self.scopes
            .last()
            .expect("scope stack should never be empty")
    }

    /// Get the current scope mutably
    pub fn current_mut(&mut self) -> &mut Scope {
        self.scopes
            .last_mut()
            .expect("scope stack should never be empty")
    }

    /// Define a variable in the current scope
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.current_mut().define(name, value);
    }

    /// Look up a variable (searches all scopes from top to bottom)
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.bindings.get(name) {
                return Some(value);
            }
            if let Some(value) = scope.imports.get(name) {
                return Some(value);
            }
        }
        None
    }

    /// Add an import to the current scope
    pub fn add_import(&mut self, alias: impl Into<String>, value: Value) {
        self.current_mut().add_import(alias, value);
    }

    /// Get an imported module
    pub fn get_import(&self, alias: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.imports.get(alias) {
                return Some(value);
            }
        }
        None
    }

    /// Get the global scope
    pub fn global(&self) -> &Scope {
        self.scopes
            .first()
            .expect("scope stack should never be empty")
    }

    /// Get the global scope mutably
    pub fn global_mut(&mut self) -> &mut Scope {
        self.scopes
            .first_mut()
            .expect("scope stack should never be empty")
    }

    /// Get all available variable names from all scopes (for error suggestions)
    pub fn available_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();

        for scope in &self.scopes {
            for key in scope.bindings.keys() {
                if !names.contains(key) {
                    names.push(key.clone());
                }
            }
            for key in scope.imports.keys() {
                if !names.contains(key) {
                    names.push(key.clone());
                }
            }
        }

        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_scope_define_and_get() {
        let mut scope = Scope::new();
        scope.define("x", Value::Int(42));

        assert_eq!(scope.get("x"), Some(&Value::Int(42)));
        assert_eq!(scope.get("y"), None);
    }

    #[test]
    fn test_scope_parent_lookup() {
        let mut parent = Scope::new();
        parent.define("x", Value::Int(42));

        let mut child = Scope::with_parent(parent);
        child.define("y", Value::Int(100));

        // Child can see parent's binding
        assert_eq!(child.get("x"), Some(&Value::Int(42)));
        // Child can see its own binding
        assert_eq!(child.get("y"), Some(&Value::Int(100)));
    }

    #[test]
    fn test_scope_shadowing() {
        let mut parent = Scope::new();
        parent.define("x", Value::Int(42));

        let mut child = Scope::with_parent(parent);
        child.define("x", Value::Int(100)); // Shadow parent's x

        // Child sees its own x
        assert_eq!(child.get("x"), Some(&Value::Int(100)));
    }

    #[test]
    fn test_scope_imports() {
        let mut scope = Scope::new();

        let mut module = IndexMap::new();
        module.insert("port".to_string(), Value::Int(8080));
        scope.add_import("utils", Value::Object(module));

        assert!(scope.get_import("utils").is_some());
        assert!(scope.get("utils").is_some());
    }

    #[test]
    fn test_scope_stack() {
        let mut stack = ScopeStack::new();

        stack.define("global_var", Value::Int(1));

        stack.push();
        stack.define("local_var", Value::Int(2));

        assert_eq!(stack.get("global_var"), Some(&Value::Int(1)));
        assert_eq!(stack.get("local_var"), Some(&Value::Int(2)));

        stack.pop();

        assert_eq!(stack.get("global_var"), Some(&Value::Int(1)));
        assert_eq!(stack.get("local_var"), None);
    }

    #[test]
    fn test_scope_stack_cannot_pop_global() {
        let mut stack = ScopeStack::new();
        assert!(stack.pop().is_none()); // Can't pop global
    }
}
