//! Integration tests for the import resolver using actual files

use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/imports")
        .join(name)
}

#[test]
fn test_resolve_file_with_from() {
    let mut resolver = hone::ImportResolver::new(fixture_path(""));
    let resolved = resolver.resolve(fixture_path("child.hone")).unwrap();

    // Should have a from dependency
    assert!(resolved.from_path.is_some());
    let from_path = resolved.from_path.as_ref().unwrap();
    assert!(from_path.ends_with("base.hone"));
}

#[test]
fn test_resolve_file_with_import() {
    let mut resolver = hone::ImportResolver::new(fixture_path(""));
    let resolved = resolver.resolve(fixture_path("main.hone")).unwrap();

    // Should have an import dependency
    assert_eq!(resolved.import_paths.len(), 1);
    assert!(resolved.import_paths[0].ends_with("utils.hone"));
}

#[test]
fn test_resolve_file_with_named_import() {
    let mut resolver = hone::ImportResolver::new(fixture_path(""));
    let resolved = resolver.resolve(fixture_path("named_import.hone")).unwrap();

    // Should have an import dependency
    assert_eq!(resolved.import_paths.len(), 1);
    assert!(resolved.import_paths[0].ends_with("utils.hone"));
}

#[test]
fn test_topological_order_real_files() {
    let mut resolver = hone::ImportResolver::new(fixture_path(""));
    let resolved = resolver.resolve(fixture_path("child.hone")).unwrap();
    let root_path = resolved.path.clone();

    let order = resolver.topological_order(&root_path).unwrap();

    // base.hone should come before child.hone
    assert_eq!(order.len(), 2);
    assert!(order[0].path.ends_with("base.hone"));
    assert!(order[1].path.ends_with("child.hone"));
}

#[test]
fn test_resolve_standalone_file() {
    let mut resolver = hone::ImportResolver::new(fixture_path(""));
    let resolved = resolver.resolve(fixture_path("base.hone")).unwrap();

    // No dependencies
    assert!(resolved.from_path.is_none());
    assert!(resolved.import_paths.is_empty());
}
