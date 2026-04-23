use std::path::PathBuf;

use peek_core::tools::*;

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_detect_pnpm() {
    let dir = fixture_dir("node-pnpm");
    assert_eq!(detect_package_manager(&dir), Some(Tool::Pnpm));
}

#[test]
fn test_detect_npm() {
    let dir = fixture_dir("node-npm");
    assert_eq!(detect_package_manager(&dir), Some(Tool::Npm));
}

#[test]
fn test_parse_package_json_scripts() {
    let dir = fixture_dir("node-pnpm");
    let scripts = parse_package_json_scripts(&dir).unwrap();
    assert_eq!(scripts.len(), 5);

    let names: Vec<&str> = scripts.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"dev"));
    assert!(names.contains(&"build"));
    assert!(names.contains(&"test"));
    assert!(names.contains(&"lint"));
    assert!(names.contains(&"start"));

    let dev = scripts.iter().find(|s| s.name == "dev").unwrap();
    assert_eq!(dev.preview, "next dev --turbo");
}

#[test]
fn test_parse_makefile_targets() {
    let dir = fixture_dir("makefile-project");
    let targets = parse_makefile_targets(&dir).unwrap();

    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"build"));
    assert!(names.contains(&"test"));
    assert!(names.contains(&"clean"));
    assert!(names.contains(&"install"));
    // Pattern rule should not be included
    assert!(!names.iter().any(|n| n.contains('%')));
}

#[test]
fn test_parse_compose_services() {
    let dir = fixture_dir("compose-project");
    let services = parse_compose_services(&dir).unwrap();

    let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"web"));
    assert!(names.contains(&"api"));
    assert!(names.contains(&"db"));
}

#[test]
fn test_parse_cargo_commands() {
    let dir = fixture_dir("cargo-project");
    let commands = parse_cargo_commands(&dir).unwrap();
    assert!(!commands.is_empty());

    let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"build"));
    assert!(names.contains(&"test"));
    assert!(names.contains(&"run"));
    assert!(names.contains(&"check"));
}

#[test]
fn test_scan_directory_pnpm() {
    let dir = fixture_dir("node-pnpm");
    let results = scan_directory(&dir);

    // Scripts registered for all 4 JS package managers
    assert_eq!(results.len(), 4);
    let pnpm = results.iter().find(|r| r.tool == Tool::Pnpm).unwrap();
    assert_eq!(pnpm.entries.len(), 5);
}

#[test]
fn test_scan_directory_no_tools() {
    let dir = fixture_dir("nonexistent");
    let results = scan_directory(&dir);
    assert!(results.is_empty());
}
