mod common;

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Duration;

use common::fake_moonraker::FakeMoonraker;
use common::fake_printer::FakePrinter;
use serde_json::{Value, json};
use wobblescope::setup::moonraker::{
    Guard, Moonraker, MoonrakerError, Timings, Verification, data_path, guard_for,
};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn fast() -> Timings {
    Timings {
        connect: ms(500),
        request: ms(500),
        status_request: ms(500),
        restart_wait: ms(2000),
        verify_wait: ms(400),
        poll: ms(20),
    }
}

struct Rig {
    fake: FakeMoonraker,
    printer: FakePrinter,
    dir: tempfile::TempDir,
}

impl Rig {
    async fn new() -> Self {
        let fake = FakeMoonraker::start().await;
        let printer = FakePrinter::new(&fake, "ready", json!({}));
        printer.install_setup_handlers(&fake, ms(100));
        let dir = tempfile::tempdir().unwrap();
        let rig = Self { fake, printer, dir };
        std::fs::create_dir_all(rig.data().join("config")).unwrap();
        std::fs::create_dir_all(rig.data().join("logs")).unwrap();
        std::fs::write(rig.data().join("moonraker.asvc"), "klipper\n").unwrap();
        rig.printer
            .set_roots(&rig.data().join("config"), &rig.data().join("logs"));
        rig
    }

    fn data(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap().join("printer_data")
    }

    fn uid(&self) -> u32 {
        std::fs::metadata(self.dir.path()).unwrap().uid()
    }

    fn client(&self, key: Option<&str>) -> Moonraker {
        Moonraker::new(Some(&self.fake.http_url()), key.map(str::to_owned), fast()).unwrap()
    }
}

fn entry(valid: bool, warnings: &[&str]) -> Value {
    json!({"is_valid": valid, "warnings": warnings, "anomalies": [], "path": "/home/pi/wobblescope"})
}

#[tokio::test]
async fn discovery_returns_the_roots_the_data_path_and_the_files_with_moonraker_conf_first() {
    let rig = Rig::new().await;
    rig.printer.set_config_files(&[
        ("moonraker.conf", &["server", "update_manager"]),
        ("extra.conf", &["update_manager helixscreen"]),
    ]);
    let found = rig.client(None).discover(rig.uid()).await.unwrap();
    assert_eq!(found.config_root, rig.data().join("config"));
    assert_eq!(found.data_path, Ok(rig.data()));
    assert_eq!(found.files[0].filename, "moonraker.conf");
    assert_eq!(found.main_config, rig.data().join("config/moonraker.conf"));
    assert!(found.has_update_manager());
    assert!(!found.wobblescope_loaded());
    assert!(!found.login_required);
}

#[tokio::test]
async fn a_section_in_an_included_file_counts_as_loaded_and_reports_its_path() {
    let rig = Rig::new().await;
    rig.printer.set_config_files(&[
        ("moonraker.conf", &["server"]),
        ("include/um.conf", &["update_manager"]),
    ]);
    rig.printer.set_entry(Some(entry(true, &[])));
    let found = rig.client(None).discover(rig.uid()).await.unwrap();
    assert!(found.wobblescope_loaded());
    assert_eq!(
        found.wobblescope_path.as_deref(),
        Some("/home/pi/wobblescope")
    );
}

#[tokio::test]
async fn a_moonraker_without_an_update_manager_is_reported() {
    let rig = Rig::new().await;
    rig.printer
        .set_config_files(&[("moonraker.conf", &["server"])]);
    let found = rig.client(None).discover(rig.uid()).await.unwrap();
    assert!(!found.has_update_manager());
}

#[tokio::test]
async fn required_logins_are_reported() {
    let rig = Rig::new().await;
    rig.printer.set_login_required(true);
    assert!(
        rig.client(None)
            .discover(rig.uid())
            .await
            .unwrap()
            .login_required
    );
}

#[tokio::test]
async fn an_unauthorized_identify_is_a_refusal_without_the_key_in_the_message() {
    let rig = Rig::new().await;
    rig.printer.require_key(Some("s3cret"));
    let refused = rig.client(None).discover(rig.uid()).await.unwrap_err();
    assert_eq!(refused, MoonrakerError::Unauthorized);
    assert!(!refused.to_string().contains("s3cret"));
    let wrong = rig
        .client(Some("wrong"))
        .discover(rig.uid())
        .await
        .unwrap_err();
    assert_eq!(wrong, MoonrakerError::Unauthorized);
    assert!(!wrong.to_string().contains("wrong"));
    rig.client(Some("s3cret"))
        .discover(rig.uid())
        .await
        .unwrap();
}

#[tokio::test]
async fn an_unreachable_moonraker_says_setup_needs_it_to_answer() {
    let rig = Rig::new().await;
    let client = rig.client(None);
    rig.fake.set_accepting(false);
    let error = client.discover(rig.uid()).await.unwrap_err();
    assert!(
        matches!(error, MoonrakerError::Unreachable { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("needs Moonraker to answer"));
}

#[tokio::test]
async fn paths_that_are_relative_hold_control_characters_or_belong_to_another_user_are_rejected() {
    let rig = Rig::new().await;
    for roots in [
        json!([{"name": "config", "path": "relative/config"}]),
        json!([{"name": "config", "path": "/tmp/a\nb/config"}]),
    ] {
        rig.printer.set_raw_roots(roots);
        let error = rig.client(None).discover(rig.uid()).await.unwrap_err();
        assert!(matches!(error, MoonrakerError::Failed(_)), "{error}");
    }
    rig.printer
        .set_roots(&rig.data().join("config"), &rig.data().join("logs"));
    let other = rig.uid().wrapping_add(1);
    let error = rig.client(None).discover(other).await.unwrap_err();
    assert!(error.to_string().contains("not owned by the invoking user"));
}

#[test]
fn the_data_path_needs_the_standard_layout_to_hold_literally() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let pd = base.join("pd");
    std::fs::create_dir_all(pd.join("config")).unwrap();
    std::fs::create_dir_all(pd.join("logs")).unwrap();
    let (config, logs) = (pd.join("config"), pd.join("logs"));
    let missing = data_path(&config, Some(&logs)).unwrap_err();
    assert!(missing.contains("moonraker.asvc"));
    std::fs::write(pd.join("moonraker.asvc"), "").unwrap();
    assert_eq!(data_path(&config, Some(&logs)), Ok(pd.clone()));
    assert!(data_path(&config, None).is_err());

    // The roots Moonraker reports for a symlinked layout sit under another parent.
    let real = base.join("real");
    std::fs::create_dir_all(real.join("cfgdir")).unwrap();
    std::fs::create_dir_all(real.join("logdir")).unwrap();
    let error = data_path(&real.join("cfgdir"), Some(&real.join("logdir"))).unwrap_err();
    assert!(error.contains("by hand"), "{error}");

    // A symlinked `config` is not the literal layout either.
    let other = base.join("other");
    std::fs::create_dir_all(other.join("logs")).unwrap();
    std::fs::write(other.join("moonraker.asvc"), "").unwrap();
    std::os::unix::fs::symlink(real.join("cfgdir"), other.join("config")).unwrap();
    assert!(data_path(&other.join("config"), Some(&other.join("logs"))).is_err());
}

#[test]
fn the_print_guard_follows_klippys_state_and_the_print_state() {
    let blocked = |k, p| matches!(guard_for(k, p), Guard::Blocked(_));
    assert!(blocked("ready", Some("printing")));
    assert!(blocked("ready", Some("paused")));
    assert!(blocked("ready", None));
    assert!(blocked("startup", None));
    assert!(blocked("disconnected", None));
    assert_eq!(guard_for("ready", Some("standby")), Guard::Clear);
    assert_eq!(guard_for("ready", Some("complete")), Guard::Clear);
    assert_eq!(guard_for("shutdown", None), Guard::Clear);
    assert_eq!(guard_for("error", None), Guard::Clear);
}

#[tokio::test]
async fn the_guard_reads_the_print_state_from_the_printer() {
    let rig = Rig::new().await;
    let client = rig.client(None);
    assert_eq!(client.print_guard().await.unwrap(), Guard::Clear);
    rig.printer
        .set_field("print_stats", "state", json!("printing"));
    assert!(matches!(
        client.print_guard().await.unwrap(),
        Guard::Blocked(_)
    ));
    rig.printer.set_state("shutdown");
    assert_eq!(client.print_guard().await.unwrap(), Guard::Clear);
    rig.printer.set_state("ready");
    rig.printer.remove_object("print_stats");
    assert!(matches!(
        client.print_guard().await.unwrap(),
        Guard::Blocked(_)
    ));
}

#[tokio::test]
async fn a_restart_waits_for_the_new_process_and_reads_its_entry() {
    let rig = Rig::new().await;
    rig.printer.set_entry(Some(entry(false, &["stale"])));
    rig.printer.set_entry_after_restart(Some(entry(true, &[])));
    let client = rig.client(None);
    client.restart().await.unwrap();
    assert_eq!(rig.printer.restarts(), 1);
    let Verification::Listed(verdict) = client.verify().await else {
        panic!("expected an entry");
    };
    assert!(verdict.valid && verdict.warnings.is_empty());
}

#[tokio::test]
async fn an_invalid_entry_carries_moonrakers_warnings() {
    let rig = Rig::new().await;
    rig.printer
        .set_entry(Some(entry(false, &["path is not a directory"])));
    let Verification::Listed(verdict) = rig.client(None).verify().await else {
        panic!("expected an entry");
    };
    assert!(!verdict.valid);
    assert_eq!(verdict.warnings, ["path is not a directory"]);
}

#[tokio::test]
async fn a_missing_entry_times_out() {
    let rig = Rig::new().await;
    assert_eq!(rig.client(None).verify().await, Verification::TimedOut);
    assert!(!rig.client(None).lists_entry().await.unwrap());
}

#[tokio::test]
async fn an_update_status_error_is_not_an_entry() {
    let rig = Rig::new().await;
    rig.fake.on(
        "machine.update.status",
        common::fake_moonraker::Script::Error(-32601, "Method not found"),
    );
    assert_eq!(rig.client(None).verify().await, Verification::TimedOut);
}

#[test]
fn the_main_config_path_comes_from_the_first_file_and_refuses_odd_names() {
    use wobblescope::setup::moonraker::main_config_path;
    let root = std::path::Path::new("/home/pi/printer_data/config");
    assert_eq!(
        main_config_path(root, None).unwrap(),
        root.join("moonraker.conf")
    );
    assert_eq!(
        main_config_path(root, Some("custom.conf")).unwrap(),
        root.join("custom.conf")
    );
    assert_eq!(
        main_config_path(root, Some("/etc/moonraker.conf")).unwrap(),
        std::path::Path::new("/etc/moonraker.conf")
    );
    assert!(main_config_path(root, Some("../x.conf")).is_err());
    assert!(main_config_path(root, Some("a\nb.conf")).is_err());
}
