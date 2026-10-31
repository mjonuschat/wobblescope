use std::collections::BTreeSet;
use std::fs;

use tempfile::TempDir;
use wobblescope::storage::pins::Pins;

fn pins_of(ids: &[&str]) -> Pins {
    let mut p = Pins::default();
    for id in ids {
        p.insert((*id).to_owned());
    }
    p
}

fn load_text(text: &str) -> Result<Pins, String> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pins.json");
    fs::write(&path, text).unwrap();
    Pins::load(&path).map_err(|e| e.to_string())
}

#[test]
fn a_missing_file_loads_as_no_pins() {
    let dir = TempDir::new().unwrap();
    let p = Pins::load(&dir.path().join("pins.json")).unwrap();
    assert!(p.is_empty());
}

#[test]
fn written_pins_load_back_sorted() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pins.json");
    pins_of(&["b", "a"]).write(&path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"version\": 1"));
    assert!(text.find("\"a\"").unwrap() < text.find("\"b\"").unwrap());
    let loaded = Pins::load(&path).unwrap();
    assert_eq!(
        loaded.iter().collect::<Vec<_>>(),
        BTreeSet::from(["a", "b"]).into_iter().collect::<Vec<_>>()
    );
}

#[test]
fn an_unknown_version_is_refused_naming_the_file_and_version() {
    let err = load_text("{\"version\":2,\"pinned\":[]}").unwrap_err();
    assert!(err.contains("pins.json") && err.contains('2'), "{err}");
}

#[test]
fn damaged_files_are_refused_naming_the_file() {
    for text in [
        "not json",
        "",
        "{\"version\":1}",
        "{\"version\":1,\"pinned\":[\"../x\"]}",
    ] {
        let err = load_text(text).expect_err(text);
        assert!(err.contains("pins.json"), "{text:?}: {err}");
    }
}

#[test]
fn a_write_leaves_no_temp_file_and_replaces_a_stale_one() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pins.json");
    let stale = dir.path().join("pins.json.tmp");
    fs::write(&stale, b"left by a crash").unwrap();
    pins_of(&["a"]).write(&path).unwrap();
    assert!(!stale.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn a_failed_write_leaves_the_existing_file_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pins.json");
    pins_of(&["a"]).write(&path).unwrap();
    let before = fs::read(&path).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let result = pins_of(&["a", "b"]).write(&path);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn pruning_drops_pins_for_missing_sessions_without_touching_the_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pins.json");
    pins_of(&["a", "b"]).write(&path).unwrap();
    let before = fs::read(&path).unwrap();
    let mut p = Pins::load(&path).unwrap();
    let dropped = p.prune(|id| id == "a");
    assert_eq!(dropped, ["b"]);
    assert!(p.contains("a") && !p.contains("b"));
    assert_eq!(fs::read(&path).unwrap(), before);
}
