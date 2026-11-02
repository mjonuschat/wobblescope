use std::io::Cursor;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Command as Process;

use clap::Parser;
use wobblescope::cli::{ApiKey, Cli, Command};
use wobblescope::setup::host::{Invoker, valid_user_name, write_atomic};
use wobblescope::setup::resolve_api_key;

fn binary() -> Process {
    let mut command = Process::new(env!("CARGO_BIN_EXE_wobblescope"));
    command
        .env_remove("SUDO_USER")
        .env_remove("WOBBLESCOPE_CONFIG");
    command
}

const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
    pi:x:1000:1001:Pi,,,:/home/pi:/bin/bash\n\
    broken line\n";

#[test]
fn no_arguments_still_start_the_daemon_path() {
    let output = binary()
        .env("WOBBLESCOPE_CONFIG", "/nonexistent/wobblescope.toml")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot read config"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn setup_without_sudo_names_the_command_to_run() {
    let output = binary().arg("setup").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sudo wobblescope setup"));
}

#[test]
fn check_does_not_ask_for_sudo() {
    let output = binary().args(["setup", "--check"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("must be run with sudo"));
}

#[test]
fn remove_and_check_together_are_refused() {
    let output = binary()
        .args(["setup", "--remove", "--check"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn help_lists_setup() {
    let output = binary().arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("setup"));
}

#[test]
fn the_key_on_standard_input_is_read_and_never_shown() {
    let cli = Cli::try_parse_from(["wobblescope", "setup", "--api-key", "-"]).unwrap();
    let Some(Command::Setup(args)) = cli.command else {
        panic!("expected setup");
    };
    let key = resolve_api_key(args.api_key.as_ref(), &mut Cursor::new("s3cret\nmore\n")).unwrap();
    assert_eq!(key.as_deref(), Some("s3cret"));
    let cli = Cli::try_parse_from(["wobblescope", "setup", "--api-key", "hunter2"]).unwrap();
    assert!(!format!("{cli:?}").contains("hunter2"));
    assert_eq!(
        resolve_api_key(Some(&ApiKey::new("k")), &mut Cursor::new("")).unwrap(),
        Some("k".to_owned())
    );
    assert!(resolve_api_key(Some(&ApiKey::new("-")), &mut Cursor::new("\n")).is_err());
}

#[test]
fn account_names_outside_the_plain_set_are_refused() {
    assert!(valid_user_name("pi"));
    assert!(valid_user_name("print_er-2"));
    for bad in ["", "-x", "a b", "a\nb", "a/b", "pi;rm"] {
        assert!(!valid_user_name(bad), "{bad:?}");
    }
    assert!(Invoker::from_sudo(PASSWD, Some("pi;rm"), None, None).is_err());
}

#[test]
fn the_passwd_lookup_finds_ids_and_home_and_fails_for_an_unknown_user() {
    let pi = Invoker::from_sudo(PASSWD, Some("pi"), None, None).unwrap();
    assert_eq!((pi.uid, pi.gid), (1000, 1001));
    assert_eq!(pi.home, std::path::Path::new("/home/pi"));
    let overridden = Invoker::from_sudo(PASSWD, Some("pi"), Some("1500"), Some("x")).unwrap();
    assert_eq!((overridden.uid, overridden.gid), (1500, 1001));
    assert!(Invoker::from_sudo(PASSWD, Some("nobody"), None, None).is_err());
    assert!(Invoker::from_sudo(PASSWD, None, None, None).is_err());
    assert_eq!(Invoker::from_uid(PASSWD, 0).unwrap().name, "root");
}

#[test]
fn the_atomic_write_replaces_contents_keeps_the_mode_and_chowns_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    write_atomic(&path, b"new", None, None).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o640);
    let me = std::fs::metadata(dir.path()).unwrap();
    write_atomic(&path, b"third", Some(0o600), Some((me.uid(), me.gid()))).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
    assert_eq!(std::fs::metadata(&path).unwrap().uid(), me.uid());
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "no temporary file is left behind");
}

#[test]
fn the_atomic_write_follows_a_symlink_to_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real");
    let link = dir.path().join("link");
    std::fs::write(&target, "old").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    write_atomic(&link, b"new", None, None).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
}
