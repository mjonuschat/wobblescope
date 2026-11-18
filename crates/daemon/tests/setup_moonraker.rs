mod common;

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Duration;

use common::fake_moonraker::FakeMoonraker;
use common::fake_printer::FakePrinter;
use serde_json::json;
use wobblescope::setup::moonraker::{Moonraker, MoonrakerError, Timings, data_path};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn fast() -> Timings {
    Timings {
        connect: ms(500),
        request: ms(500),
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

#[tokio::test]
async fn discovery_returns_the_config_root_and_the_data_path() {
    let rig = Rig::new().await;
    let found = rig.client(None).discover(rig.uid()).await.unwrap();
    assert_eq!(found.config_root, rig.data().join("config"));
    assert_eq!(found.data_path, Ok(rig.data()));
    assert!(!found.login_required);
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
fn setup_refuses_an_https_moonraker_url() {
    let Err(err) = Moonraker::new(Some("https://printer.local:7130"), None, fast()) else {
        panic!("an https URL must be refused");
    };
    assert!(err.to_string().contains("plain http"), "{err}");
}
