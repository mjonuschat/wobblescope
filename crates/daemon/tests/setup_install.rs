mod common;

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::fake_moonraker::FakeMoonraker;
use common::fake_printer::FakePrinter;
use serde_json::json;
use wobblescope::setup::host::{Host, Invoker, Outcome, Runner};
use wobblescope::setup::moonraker::{Moonraker, Timings};
use wobblescope::setup::{Mode, Options, SetupError, execute};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[derive(Default)]
struct Systemd {
    calls: Vec<String>,
    enabled: bool,
    active: bool,
    flaps: bool,
    fail_on: Option<String>,
}

#[derive(Clone, Default)]
struct FakeSystemd(Arc<Mutex<Systemd>>);

impl FakeSystemd {
    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }

    fn count(&self, call: &str) -> usize {
        self.calls().iter().filter(|c| *c == call).count()
    }
}

impl Runner for FakeSystemd {
    fn run(&self, _program: &str, args: &[&str]) -> std::io::Result<Outcome> {
        let mut s = self.0.lock().unwrap();
        let call = args.join(" ");
        s.calls.push(call.clone());
        let done = |success: bool, stdout: &str| {
            Ok(Outcome {
                success,
                stdout: stdout.to_owned(),
                stderr: String::new(),
            })
        };
        if s.fail_on.as_deref() == Some(call.as_str()) {
            s.fail_on = None;
            return done(false, "");
        }
        match args {
            ["is-enabled", _] => done(s.enabled, if s.enabled { "enabled" } else { "disabled" }),
            ["is-active", _] if s.active && s.flaps => done(false, "activating"),
            ["is-active", _] => done(s.active, if s.active { "active" } else { "inactive" }),
            ["enable", _] => {
                s.enabled = true;
                done(true, "")
            }
            ["restart" | "start", _] => {
                s.active = true;
                done(true, "")
            }
            ["disable", "--now", _] => {
                s.enabled = false;
                s.active = false;
                done(true, "")
            }
            _ => done(true, ""),
        }
    }
}

struct Rig {
    fake: FakeMoonraker,
    printer: FakePrinter,
    systemd: FakeSystemd,
    root: tempfile::TempDir,
    binary: PathBuf,
}

const CONF: &str = "[server]\nhost: 0.0.0.0\n\n[include extra.conf]\n\n[update_manager]\n";

impl Rig {
    async fn new() -> Self {
        let fake = FakeMoonraker::start().await;
        let printer = FakePrinter::new(&fake, "ready", json!({}));
        printer.install_setup_handlers(&fake, ms(60));
        let root = tempfile::tempdir().unwrap();
        let rig = Self {
            binary: root
                .path()
                .canonicalize()
                .unwrap()
                .join("home/pi/wobblescope/wobblescope-x86_64"),
            fake,
            printer,
            systemd: FakeSystemd::default(),
            root,
        };
        let data = rig.data();
        std::fs::create_dir_all(data.join("config")).unwrap();
        std::fs::create_dir_all(data.join("logs")).unwrap();
        std::fs::create_dir_all(rig.install_dir()).unwrap();
        std::fs::create_dir_all(rig.unit_dir()).unwrap();
        std::fs::write(data.join("moonraker.asvc"), "klipper\nmoonraker\n").unwrap();
        std::fs::write(data.join("config/moonraker.conf"), CONF).unwrap();
        std::fs::write(&rig.binary, "BINARY v1").unwrap();
        rig.printer
            .set_roots(&data.join("config"), &data.join("logs"));
        rig
    }

    fn base(&self) -> PathBuf {
        self.root.path().canonicalize().unwrap()
    }
    fn data(&self) -> PathBuf {
        self.base().join("printer_data")
    }
    fn home(&self) -> PathBuf {
        self.base().join("home/pi")
    }
    fn install_dir(&self) -> PathBuf {
        self.home().join("wobblescope")
    }
    fn unit_dir(&self) -> PathBuf {
        self.base().join("systemd")
    }
    fn conf(&self) -> PathBuf {
        self.data().join("config/moonraker.conf")
    }
    fn asvc(&self) -> PathBuf {
        self.data().join("moonraker.asvc")
    }
    fn config(&self) -> PathBuf {
        self.data().join("config/wobblescope.toml")
    }
    fn host(&self) -> Host {
        let meta = std::fs::metadata(self.root.path()).unwrap();
        Host {
            invoker: Invoker {
                name: "pi".to_owned(),
                uid: meta.uid(),
                gid: meta.gid(),
                home: self.home(),
            },
            chown: false,
            unit_dir: self.unit_dir(),
            binary: self.binary.clone(),
            runner: Box::new(self.systemd.clone()),
            settle: Duration::ZERO,
        }
    }

    fn options(&self, mode: Mode, key: Option<&str>) -> Options {
        Options {
            mode,
            moonraker: Some(self.fake.http_url()),
            api_key: key.map(str::to_owned),
        }
    }

    async fn run(&self, mode: Mode, key: Option<&str>) -> (Result<bool, SetupError>, String) {
        let timings = Timings {
            connect: ms(500),
            request: ms(500),
        };
        let moonraker =
            Moonraker::new(Some(&self.fake.http_url()), key.map(str::to_owned), timings).unwrap();
        let mut out = Vec::new();
        let result = execute(&self.host(), &moonraker, &self.options(mode, key), &mut out).await;
        (result, String::from_utf8(out).unwrap())
    }

    async fn install(&self) -> (Result<bool, SetupError>, String) {
        self.run(Mode::Install, None).await
    }

    fn unit_path(&self) -> PathBuf {
        self.unit_dir().join("wobblescope.service")
    }
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[tokio::test]
async fn a_first_install_writes_the_unit_the_config_and_the_service_list_entry_and_nothing_else() {
    let rig = Rig::new().await;
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(std::fs::read_to_string(&rig.binary).unwrap(), "BINARY v1");
    assert!(rig.config().exists());
    assert_eq!(mode_of(&rig.config()), 0o644);
    let unit = std::fs::read_to_string(rig.unit_path()).unwrap();
    assert!(unit.contains(&rig.binary.display().to_string()));
    assert!(unit.contains(&rig.config().display().to_string()));
    assert!(
        std::fs::read_to_string(rig.asvc())
            .unwrap()
            .lines()
            .any(|l| l == "wobblescope")
    );
    assert!(out.contains("restart Moonraker"), "{out}");
    assert_eq!(std::fs::read_to_string(rig.conf()).unwrap(), CONF);
    assert!(!rig.install_dir().join("release_info.json").exists());
    let systemd = &rig.systemd;
    assert_eq!(systemd.count("daemon-reload"), 1);
    assert_eq!(systemd.count("enable wobblescope"), 1);
    assert_eq!(systemd.count("restart wobblescope"), 1);
    assert_eq!(rig.printer.restarts(), 0);
    let (check, report) = rig.run(Mode::Check, None).await;
    assert_eq!(check, Ok(true), "{report}");
}

#[tokio::test]
async fn a_second_run_finds_the_files_current_and_restarts_the_service_onto_the_installed_binary() {
    let rig = Rig::new().await;
    rig.install().await.0.unwrap();
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(rig.systemd.count("restart wobblescope"), 2);
    assert_eq!(rig.systemd.count("enable wobblescope"), 1);
    assert_eq!(rig.printer.restarts(), 0);
    assert!(
        out.lines()
            .any(|l| l.contains("service unit") && l.contains("already current")),
        "{out}"
    );
}

#[tokio::test]
async fn an_unauthorized_moonraker_stops_setup_before_it_writes_anything() {
    let rig = Rig::new().await;
    rig.printer.require_key(Some("s3cret"));
    let (result, out) = rig.install().await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("--api-key"), "{error}");
    assert!(!error.contains("s3cret") && !out.contains("s3cret"));
    assert_eq!(std::fs::read_dir(rig.unit_dir()).unwrap().count(), 0);
    assert!(!rig.config().exists());
    assert_eq!(std::fs::read_to_string(rig.conf()).unwrap(), CONF);
    assert!(rig.systemd.calls().is_empty());

    let (result, out) = rig.run(Mode::Install, Some("s3cret")).await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(mode_of(&rig.config()), 0o600);
    let config: toml::Table = std::fs::read_to_string(rig.config())
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config["moonraker_api_key"].as_str(), Some("s3cret"));
    assert!(!out.contains("s3cret"));
}

#[tokio::test]
async fn an_existing_config_file_is_left_alone_and_the_flags_are_reported_as_not_applied() {
    let rig = Rig::new().await;
    rig.printer.require_key(Some("k"));
    let mine = "listen = \"127.0.0.1:9000\"\n";
    std::fs::write(rig.config(), mine).unwrap();
    let (result, out) = rig.run(Mode::Install, Some("k")).await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(std::fs::read_to_string(rig.config()).unwrap(), mine);
    assert!(out.contains("--api-key not applied"), "{out}");
    assert!(out.contains("--moonraker not applied"), "{out}");
}

#[tokio::test]
async fn check_names_a_missing_piece_changes_nothing_and_runs_only_read_commands() {
    let rig = Rig::new().await;
    rig.install().await.0.unwrap();
    let asvc = std::fs::read_to_string(rig.asvc()).unwrap();
    let broken = asvc.replace("wobblescope\n", "");
    std::fs::write(rig.asvc(), &broken).unwrap();
    let before = rig.systemd.calls().len();
    let (result, out) = rig.run(Mode::Check, None).await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(
        out.lines()
            .any(|l| l.contains("moonraker.asvc") && l.contains("missing")),
        "{out}"
    );
    assert_eq!(std::fs::read_to_string(rig.asvc()).unwrap(), broken);
    let calls = rig.systemd.calls();
    assert!(
        calls[before..]
            .iter()
            .all(|c| c.starts_with("is-enabled") || c.starts_with("is-active")),
        "{calls:?}"
    );
    let (repair, _) = rig.install().await;
    assert_eq!(repair, Ok(true));
    assert_eq!(std::fs::read_to_string(rig.asvc()).unwrap(), asvc);
}

#[tokio::test]
async fn remove_takes_out_the_service_and_its_list_entry_and_keeps_the_rest() {
    let rig = Rig::new().await;
    rig.install().await.0.unwrap();
    let (result, out) = rig.run(Mode::Remove, None).await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(std::fs::read_to_string(rig.conf()).unwrap(), CONF);
    assert_eq!(
        std::fs::read_to_string(rig.asvc()).unwrap(),
        "klipper\nmoonraker\n"
    );
    assert!(!rig.unit_path().exists());
    assert!(rig.binary.exists());
    assert!(rig.config().exists());
    assert_eq!(rig.printer.restarts(), 0);
    assert_eq!(rig.systemd.count("disable --now wobblescope"), 1);
    assert!(out.contains("restart Moonraker"), "{out}");
}

#[tokio::test]
async fn a_failed_service_restart_stops_the_run_and_a_rerun_restarts_it() {
    let rig = Rig::new().await;
    rig.systemd.0.lock().unwrap().fail_on = Some("restart wobblescope".to_owned());
    assert!(rig.install().await.0.is_err());
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(rig.systemd.count("restart wobblescope"), 2);
}

#[tokio::test]
async fn a_service_that_does_not_stay_active_fails_the_run_with_a_journal_hint() {
    let rig = Rig::new().await;
    rig.systemd.0.lock().unwrap().flaps = true;
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(out.contains("journalctl -u wobblescope"), "{out}");
}

#[tokio::test]
async fn a_failing_enable_stops_the_run_and_the_report_lists_the_steps_already_done() {
    let rig = Rig::new().await;
    rig.systemd.0.lock().unwrap().fail_on = Some("enable wobblescope".to_owned());
    let (result, out) = rig.install().await;
    assert!(result.is_err());
    assert!(out.contains("service unit"), "{out}");
    assert!(!rig.config().exists());
}

#[tokio::test]
async fn required_logins_print_a_warning_and_the_run_still_finishes() {
    let rig = Rig::new().await;
    rig.printer.set_login_required(true);
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert!(out.contains("force_logins"), "{out}");
}
