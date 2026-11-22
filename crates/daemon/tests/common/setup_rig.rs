use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::fake_moonraker::FakeMoonraker;
use super::fake_printer::FakePrinter;
use serde_json::json;
use wobblescope::setup::host::{Host, Invoker, Outcome, Runner};
use wobblescope::setup::moonraker::{Moonraker, Timings};
use wobblescope::setup::{Mode, Options, SetupError, execute};

pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[derive(Default)]
pub struct Systemd {
    pub calls: Vec<String>,
    pub enabled: bool,
    pub active: bool,
    pub flaps: bool,
    pub fail_on: Option<String>,
    pub nginx_test_error: Option<String>,
    /// `nginx` is not on `PATH`, but `/usr/sbin/nginx` runs.
    pub nginx_off_path: bool,
    /// No nginx anywhere.
    pub nginx_missing: bool,
}

#[derive(Clone, Default)]
pub struct FakeSystemd(pub Arc<Mutex<Systemd>>);

impl FakeSystemd {
    pub fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }

    pub fn count(&self, call: &str) -> usize {
        self.calls().iter().filter(|c| *c == call).count()
    }
}

impl Runner for FakeSystemd {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<Outcome> {
        let mut s = self.0.lock().unwrap();
        if matches!(program, "nginx" | "/usr/sbin/nginx" | "/sbin/nginx") {
            let absent = s.nginx_missing
                || (program == "nginx" && s.nginx_off_path)
                || (program == "/sbin/nginx" && s.nginx_off_path);
            if absent {
                return Err(std::io::ErrorKind::NotFound.into());
            }
            s.calls.push(format!("nginx {}", args.join(" ")));
            let error = if args == ["-t"] {
                s.nginx_test_error.clone()
            } else {
                None
            };
            return Ok(Outcome {
                success: error.is_none(),
                stdout: String::new(),
                stderr: error.unwrap_or_default(),
            });
        }
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

pub struct Rig {
    pub fake: FakeMoonraker,
    pub printer: FakePrinter,
    pub systemd: FakeSystemd,
    pub root: tempfile::TempDir,
    pub binary: PathBuf,
    /// The nginx directory the host reads. It is absent unless a test builds one.
    pub nginx_dir: PathBuf,
}

pub const CONF: &str = "[server]\nhost: 0.0.0.0\n\n[include extra.conf]\n\n[update_manager]\n";

impl Rig {
    pub async fn new() -> Self {
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
            nginx_dir: root.path().canonicalize().unwrap().join("etc-nginx"),
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

    pub fn moonraker_port(&self) -> u16 {
        self.fake
            .http_url()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }
    pub fn base(&self) -> PathBuf {
        self.root.path().canonicalize().unwrap()
    }
    pub fn data(&self) -> PathBuf {
        self.base().join("printer_data")
    }
    pub fn home(&self) -> PathBuf {
        self.base().join("home/pi")
    }
    pub fn install_dir(&self) -> PathBuf {
        self.home().join("wobblescope")
    }
    pub fn unit_dir(&self) -> PathBuf {
        self.base().join("systemd")
    }
    pub fn conf(&self) -> PathBuf {
        self.data().join("config/moonraker.conf")
    }
    pub fn asvc(&self) -> PathBuf {
        self.data().join("moonraker.asvc")
    }
    pub fn config(&self) -> PathBuf {
        self.data().join("config/wobblescope.toml")
    }
    pub fn host(&self) -> Host {
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
            nginx_dir: self.nginx_dir.clone(),
            binary: self.binary.clone(),
            runner: Box::new(self.systemd.clone()),
            settle: Duration::ZERO,
        }
    }

    pub fn options(&self, mode: Mode, key: Option<&str>) -> Options {
        Options {
            mode,
            nginx: true,
            moonraker: Some(self.fake.http_url()),
            api_key: key.map(str::to_owned),
        }
    }

    pub fn moonraker(&self) -> Moonraker {
        let timings = Timings {
            connect: ms(500),
            request: ms(500),
        };
        Moonraker::new(Some(&self.fake.http_url()), None, timings).unwrap()
    }

    pub async fn run(&self, mode: Mode, key: Option<&str>) -> (Result<bool, SetupError>, String) {
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

    pub async fn install(&self) -> (Result<bool, SetupError>, String) {
        self.run(Mode::Install, None).await
    }

    pub fn unit_path(&self) -> PathBuf {
        self.unit_dir().join("wobblescope.service")
    }
}

pub fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}
