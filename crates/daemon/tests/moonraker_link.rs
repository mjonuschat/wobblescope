mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::fake_moonraker::{FakeMoonraker, Script, eventually};
use serde_json::{Value, json};
use wobblescope::config::Settings;
use wobblescope::moonraker::status::{KlippyState, MoonrakerState, state_line};
use wobblescope::moonraker::{Moonraker, Timings, spawn};

const DEADLINE: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/data/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn fast() -> Timings {
    Timings {
        connect: ms(500),
        request: ms(200),
        configfile: ms(800),
        poll_ready: ms(20),
        poll_not_ready: ms(10),
        backoff_start: ms(10),
        backoff_max: ms(40),
    }
}

fn settings(fake: &FakeMoonraker, key: Option<&str>) -> Settings {
    Settings {
        moonraker_url: fake.http_url().parse().unwrap(),
        moonraker_api_key: key.map(str::to_owned),
        ..Settings::default()
    }
}

/// A raw config with one chip, `adxl345`, and `accel_chip` naming it.
fn one_chip() -> Value {
    json!({
        "adxl345": {"cs_pin": "PA4"},
        "resonance_tester": {"accel_chip": "adxl345"},
    })
}

fn two_chips() -> Value {
    json!({
        "adxl345": {"cs_pin": "PA4"},
        "lis2dw ebb36": {"cs_pin": "ebb36: PA4"},
        "resonance_tester": {"accel_chip": "adxl345"},
    })
}

/// What the fake's printer currently looks like; tests change it while the daemon runs.
#[derive(Clone)]
struct Printer {
    klippy_state: Arc<Mutex<String>>,
    message: Arc<Mutex<String>>,
    config: Arc<Mutex<Value>>,
}

impl Printer {
    fn new(fake: &FakeMoonraker, state: &str, config: Value) -> Self {
        let printer = Self {
            klippy_state: Arc::new(Mutex::new(state.to_owned())),
            message: Arc::new(Mutex::new("Printer is ready".to_owned())),
            config: Arc::new(Mutex::new(config)),
        };
        fake.on(
            "server.connection.identify",
            Script::Result(json!({"connection_id": 1})),
        );
        let p = printer.clone();
        fake.on_fn("server.info", move |_, _| {
            let mut info = fixture("server_info_ready.json");
            info["klippy_state"] = json!(*p.klippy_state.lock().unwrap());
            Script::Result(info)
        });
        let p = printer.clone();
        fake.on_fn("printer.info", move |_, _| {
            let mut info = fixture("printer_info_shutdown.json");
            info["state"] = json!(*p.klippy_state.lock().unwrap());
            info["state_message"] = json!(*p.message.lock().unwrap());
            Script::Result(info)
        });
        let p = printer.clone();
        fake.on_fn("printer.objects.query", move |_, _| {
            let config = p.config.lock().unwrap().clone();
            Script::Result(json!({"eventtime": 1.0, "status": {"configfile": {"config": config}}}))
        });
        fake.on(
            "server.files.roots",
            Script::Result(fixture("files_roots.json")),
        );
        printer
    }

    fn set_state(&self, state: &str) {
        *self.klippy_state.lock().unwrap() = state.to_owned();
    }

    fn set_message(&self, message: &str) {
        *self.message.lock().unwrap() = message.to_owned();
    }

    fn set_config(&self, config: Value) {
        *self.config.lock().unwrap() = config;
    }
}

async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    eventually(what, DEADLINE, &mut condition).await;
}

fn chip_sections(m: &Moonraker) -> Vec<String> {
    m.chips()
        .list
        .chips
        .iter()
        .map(|c| c.section.clone())
        .collect()
}

#[tokio::test]
async fn a_ready_printer_gives_a_connected_status_and_current_chips() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("ready and current chips", || {
        m.status().klippy.state == KlippyState::Ready && m.chips().current
    })
    .await;
    let status = m.status();
    assert_eq!(status.moonraker.state, MoonrakerState::Connected);
    assert_eq!(
        status.moonraker.version.as_deref(),
        Some("v0.9.3-64-g1d4a4b0")
    );
    let chips = m.chips();
    assert!(chips.known);
    assert_eq!(chip_sections(&m), ["adxl345"]);
    assert_eq!(chips.list.defaults.x, ["adxl345"]);
    assert_eq!(chips.list.defaults.y, ["adxl345"]);
}

#[tokio::test]
async fn identify_names_the_client_and_carries_the_key_only_when_one_is_set() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let (_m, _) = spawn(&settings(&fake, None), fast(), false);
    until("identify", || {
        !fake.requests_for("server.connection.identify").is_empty()
    })
    .await;
    let params = fake.requests_for("server.connection.identify")[0]
        .params
        .clone();
    assert_eq!(params["client_name"], "wobblescope");
    assert_eq!(params["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(params["type"], "other");
    assert_eq!(params["url"], "https://wobblescope.invalid/");
    assert!(params.get("api_key").is_none());

    let keyed = FakeMoonraker::start().await;
    Printer::new(&keyed, "ready", one_chip());
    let (_m, _) = spawn(&settings(&keyed, Some("s3cret-key")), fast(), false);
    until("identify with a key", || {
        !keyed.requests_for("server.connection.identify").is_empty()
    })
    .await;
    assert_eq!(
        keyed.requests_for("server.connection.identify")[0].params["api_key"],
        "s3cret-key"
    );
}

#[tokio::test]
async fn a_klipper_restart_that_adds_a_chip_shows_in_the_chips_once_ready_again() {
    let fake = FakeMoonraker::start().await;
    let printer = Printer::new(&fake, "ready", one_chip());
    let mut timings = fast();
    timings.poll_ready = Duration::from_secs(10);
    let (m, _) = spawn(&settings(&fake, None), timings, false);
    until("current chips", || m.chips().current).await;
    assert_eq!(chip_sections(&m), ["adxl345"]);

    printer.set_state("disconnected");
    fake.push("notify_klippy_disconnected", json!([]));
    until("disconnected", || {
        m.status().klippy.state == KlippyState::Disconnected
    })
    .await;
    assert!(m.chips().known);
    assert!(!m.chips().current);

    printer.set_state("startup");
    printer.set_message("Printer is not ready");
    until("startup with its message", || {
        let k = m.status().klippy;
        k.state == KlippyState::Startup && k.message.as_deref() == Some("Printer is not ready")
    })
    .await;

    printer.set_config(two_chips());
    printer.set_state("ready");
    fake.push("notify_klippy_ready", json!([]));
    until("both chips current", || {
        m.chips().current && chip_sections(&m).len() == 2
    })
    .await;
    assert_eq!(m.status().klippy.state, KlippyState::Ready);
}

#[tokio::test]
async fn a_repeated_ready_notification_sends_no_second_chip_query() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("current chips", || m.chips().current).await;
    fake.push("notify_klippy_ready", json!([]));
    tokio::time::sleep(ms(150)).await;
    assert_eq!(fake.requests_for("printer.objects.query").len(), 1);
}

#[tokio::test]
async fn a_poll_that_sees_startup_become_ready_rereads_the_chips() {
    let fake = FakeMoonraker::start().await;
    let printer = Printer::new(&fake, "startup", one_chip());
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("startup", || {
        m.status().klippy.state == KlippyState::Startup
    })
    .await;
    assert!(!m.chips().known);
    printer.set_state("ready");
    until("current chips", || m.chips().current).await;
    assert_eq!(m.status().klippy.state, KlippyState::Ready);
}

#[tokio::test]
async fn a_shutdown_carries_klippers_own_message() {
    let fake = FakeMoonraker::start().await;
    let printer = Printer::new(&fake, "shutdown", one_chip());
    printer.set_message("MCU 'mcu' shutdown: Timer too close");
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("the shutdown message", || {
        m.status().klippy.message.as_deref() == Some("MCU 'mcu' shutdown: Timer too close")
    })
    .await;
    assert_eq!(m.status().klippy.state, KlippyState::Shutdown);
}

#[tokio::test]
async fn a_klippy_state_the_daemon_does_not_know_is_unknown() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "bogus", one_chip());
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("connected", || {
        m.status().moonraker.state == MoonrakerState::Connected
    })
    .await;
    tokio::time::sleep(ms(50)).await;
    assert_eq!(m.status().klippy.state, KlippyState::Unknown);
}

#[tokio::test]
async fn losing_moonraker_shows_unreachable_and_its_return_makes_the_chips_current_again() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("current chips", || m.chips().current).await;

    fake.set_accepting(false);
    fake.close_connections();
    until("unreachable", || {
        m.status().moonraker.state == MoonrakerState::Unreachable
    })
    .await;
    let status = m.status();
    assert_eq!(status.klippy.state, KlippyState::Unknown);
    assert!(m.chips().known && !m.chips().current);
    assert!(status.moonraker.message.is_some());

    fake.set_accepting(true);
    until("connected and current", || {
        m.status().moonraker.state == MoonrakerState::Connected && m.chips().current
    })
    .await;
    assert_eq!(m.status().klippy.state, KlippyState::Ready);
    assert_eq!(fake.requests_for("printer.objects.query").len(), 2);
}

#[tokio::test]
async fn backoff_doubles_to_its_cap_and_resets_after_a_successful_bootstrap() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    fake.set_accepting(false);
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("five attempts", || fake.attempts().len() >= 5).await;
    fake.set_accepting(true);
    until("connected", || {
        m.status().moonraker.state == MoonrakerState::Connected
    })
    .await;
    let before = fake.attempts().len();
    fake.set_accepting(false);
    fake.close_connections();
    until("attempts after the drop", || {
        fake.attempts().len() >= before + 2
    })
    .await;

    let times = fake.attempts();
    let gaps: Vec<u128> = times
        .windows(2)
        .map(|w| (w[1] - w[0]).as_millis())
        .collect();
    // Gaps include connection time; allow slack above, none below.
    assert!(
        gaps[0] >= 10 && gaps[1] >= 20 && gaps[2] >= 40 && gaps[3] >= 40,
        "{gaps:?}"
    );
    assert!(gaps[1] > gaps[0] && gaps[2] > gaps[1], "{gaps:?}");
    assert!(gaps[3] < 80, "the cap holds: {gaps:?}");
    let after_reset = gaps[before - 1];
    assert!((10..35).contains(&after_reset), "backoff reset: {gaps:?}");
}

#[tokio::test]
async fn an_unauthorized_call_gives_unauthorized_naming_both_fixes_and_retries() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    fake.on("server.info", Script::error(-32602, "Unauthorized"));
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("unauthorized", || {
        m.status().moonraker.state == MoonrakerState::Unauthorized
    })
    .await;
    let message = m.status().moonraker.message.unwrap();
    assert!(message.contains("trusted_clients") && message.contains("moonraker_api_key"));
    until("another attempt", || {
        fake.requests_for("server.info").len() >= 2
    })
    .await;
}

#[tokio::test]
async fn a_wrong_key_is_unauthorized_and_the_key_appears_nowhere() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    fake.on(
        "server.connection.identify",
        Script::error(-32602, "Invalid API Key"),
    );
    let (m, _) = spawn(&settings(&fake, Some("s3cret-key")), fast(), false);
    until("unauthorized", || {
        m.status().moonraker.state == MoonrakerState::Unauthorized
    })
    .await;
    let status = m.status();
    assert!(!format!("{status:?}").contains("s3cret-key"));
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("s3cret-key")
    );
}

#[test]
fn the_log_line_escapes_control_characters_from_klipper() {
    let line = state_line("klipper", "shutdown", Some("bad\nforged: ready\u{1b}[31m"));
    assert!(!line.contains('\n') && !line.contains('\u{1b}'), "{line:?}");
    assert!(line.contains("\\n") && line.contains("\\u{1b}"), "{line}");
    assert!(line.starts_with("klipper: shutdown"));
}

#[tokio::test]
async fn a_slow_chip_query_within_its_own_timeout_still_gives_current_chips() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let inner = fixture("configfile_mainline_xy.json");
    // Longer than the 200 ms request timeout, shorter than the 800 ms config timeout.
    fake.on(
        "printer.objects.query",
        Script::after(ms(400), Script::Result(inner)),
    );
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("current chips", || m.chips().current).await;
    assert_eq!(m.status().moonraker.state, MoonrakerState::Connected);
    assert_eq!(chip_sections(&m), ["adxl345 nh36", "lis2dw ebb36"]);
}

#[tokio::test]
async fn a_chip_query_past_its_timeout_keeps_the_link_and_is_tried_again_next_poll() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let good = json!({"status": {"configfile": {"config": one_chip()}}});
    fake.on_fn("printer.objects.query", move |_, n| {
        if n == 0 {
            Script::after(ms(2000), Script::Result(json!({})))
        } else {
            Script::Result(good.clone())
        }
    });
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("a second query", || {
        fake.requests_for("printer.objects.query").len() >= 2
    })
    .await;
    assert_eq!(m.status().moonraker.state, MoonrakerState::Connected);
    assert_eq!(fake.connections(), 1);
    until("current chips", || m.chips().current).await;
}

#[tokio::test]
async fn a_shutdown_during_a_slow_chip_read_shows_at_once() {
    let fake = FakeMoonraker::start().await;
    let printer = Printer::new(&fake, "ready", one_chip());
    fake.on("printer.objects.query", Script::Silent);
    let mut timings = fast();
    timings.configfile = Duration::from_secs(5);
    timings.poll_ready = Duration::from_secs(10);
    let (m, _) = spawn(&settings(&fake, None), timings, false);
    until("the chip query", || {
        !fake.requests_for("printer.objects.query").is_empty()
    })
    .await;
    printer.set_state("shutdown");
    let pushed = std::time::Instant::now();
    fake.push("notify_klippy_shutdown", json!([]));
    until("shutdown", || {
        m.status().klippy.state == KlippyState::Shutdown
    })
    .await;
    assert!(
        pushed.elapsed() < Duration::from_secs(2),
        "waited for the chip read"
    );
    assert!(!m.chips().current);
}

#[tokio::test]
async fn a_read_that_a_restart_overtakes_is_not_current_and_is_read_again() {
    let fake = FakeMoonraker::start().await;
    let printer = Printer::new(&fake, "ready", one_chip());
    let fresh = json!({"status": {"configfile": {"config": two_chips()}}});
    let stale = json!({"status": {"configfile": {"config": one_chip()}}});
    fake.on_fn("printer.objects.query", move |_, n| {
        if n == 0 {
            Script::after(ms(300), Script::Result(stale.clone()))
        } else {
            Script::Result(fresh.clone())
        }
    });
    let mut timings = fast();
    timings.poll_ready = Duration::from_secs(10);
    let (m, _) = spawn(&settings(&fake, None), timings, false);
    until("the first chip query", || {
        !fake.requests_for("printer.objects.query").is_empty()
    })
    .await;
    printer.set_config(two_chips());
    fake.push("notify_klippy_disconnected", json!([]));
    fake.push("notify_klippy_ready", json!([]));
    until("both chips current", || {
        m.chips().current && chip_sections(&m).len() == 2
    })
    .await;
    assert_eq!(fake.requests_for("printer.objects.query").len(), 2);
}

#[tokio::test]
async fn a_chip_query_that_errors_keeps_the_link_and_the_next_poll_recovers() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let good = json!({"status": {"configfile": {"config": one_chip()}}});
    fake.on_fn("printer.objects.query", move |_, n| {
        if n == 0 {
            Script::error(-32601, "Method not found")
        } else {
            Script::Result(good.clone())
        }
    });
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("current chips", || m.chips().current).await;
    assert_eq!(m.status().moonraker.state, MoonrakerState::Connected);
    assert_eq!(fake.connections(), 1);
    assert!(fake.requests_for("printer.objects.query").len() >= 2);
}

#[tokio::test]
async fn a_poll_that_times_out_ends_the_connection() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    fake.on_fn("server.info", |_, n| {
        if n == 0 {
            Script::Result(fixture("server_info_ready.json"))
        } else {
            Script::Silent
        }
    });
    let (m, _) = spawn(&settings(&fake, None), fast(), false);
    until("unreachable", || {
        m.status().moonraker.state == MoonrakerState::Unreachable
    })
    .await;
    assert!(m.status().moonraker.message.unwrap().contains("timed out"));
    until("a new connection", || fake.connections() >= 2).await;
}

#[tokio::test]
async fn the_config_root_is_reported_without_waiting_for_ready_and_asked_once() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "error", one_chip());
    let (m, report) = spawn(&settings(&fake, None), fast(), true);
    let root = tokio::time::timeout(DEADLINE, report.unwrap())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        root,
        std::path::PathBuf::from("/home/pi/printer_data/config")
    );
    until("klipper's error state", || {
        m.status().klippy.state == KlippyState::Error
    })
    .await;

    for _ in 0..2 {
        let seen = fake.connections();
        fake.close_connections();
        until("a reconnect", || fake.connections() > seen).await;
    }
    assert_eq!(fake.requests_for("server.files.roots").len(), 1);
}

#[tokio::test]
async fn roots_without_a_config_entry_are_reported_as_an_error() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    fake.on(
        "server.files.roots",
        Script::Result(json!([{"name": "gcodes", "path": "/g", "permissions": "rw"}])),
    );
    let (_m, report) = spawn(&settings(&fake, None), fast(), true);
    let answer = tokio::time::timeout(DEADLINE, report.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(answer.is_err());
}

#[tokio::test]
async fn without_asking_for_the_root_the_fake_never_sees_the_roots_call() {
    let fake = FakeMoonraker::start().await;
    Printer::new(&fake, "ready", one_chip());
    let (m, report) = spawn(&settings(&fake, None), fast(), false);
    assert!(report.is_none());
    until("current chips", || m.chips().current).await;
    assert!(fake.requests_for("server.files.roots").is_empty());
}
