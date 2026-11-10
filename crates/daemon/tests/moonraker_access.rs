mod common;

use std::net::IpAddr;
use std::time::Duration;

use common::fake_moonraker::{FakeMoonraker, Script, eventually};
use common::fake_printer::{FakePrinter, fixture};
use serde_json::json;
use wobblescope::access::AccessState;
use wobblescope::config::Settings;
use wobblescope::moonraker::status::MoonrakerState;
use wobblescope::moonraker::{Moonraker, Timings, spawn};

const DEADLINE: Duration = Duration::from_secs(10);

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

fn settings(fake: &FakeMoonraker) -> Settings {
    Settings {
        moonraker_url: fake.http_url().parse().unwrap(),
        ..Settings::default()
    }
}

fn chips() -> serde_json::Value {
    json!({"adxl345": {"cs_pin": "PA4"}, "resonance_tester": {"accel_chip": "adxl345"}})
}

async fn rig() -> (FakeMoonraker, FakePrinter, Moonraker) {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", chips());
    let (m, _) = spawn(&settings(&fake), fast(), false);
    (fake, printer, m)
}

async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    eventually(what, DEADLINE, &mut condition).await;
}

fn known(m: &Moonraker) -> bool {
    matches!(m.access(), AccessState::Known(_))
}

fn logins_required(m: &Moonraker) -> Option<bool> {
    match m.access() {
        AccessState::Known(policy) => Some(policy.logins_required()),
        AccessState::Unknown { .. } => None,
    }
}

fn hostname(m: &Moonraker) -> Option<String> {
    match m.access() {
        AccessState::Known(policy) => policy.hostname().map(str::to_owned),
        AccessState::Unknown { .. } => None,
    }
}

#[tokio::test]
async fn the_policy_becomes_known_with_the_lists_and_the_hostname() {
    let (fake, _printer, m) = rig().await;
    until("a hostname", || hostname(&m).is_some()).await;
    let AccessState::Known(policy) = m.access() else {
        panic!("not known");
    };
    assert!(policy.is_trusted("192.168.1.20".parse::<IpAddr>().unwrap()));
    assert!(!policy.is_trusted("203.0.113.9".parse::<IpAddr>().unwrap()));
    assert_eq!(policy.hostname(), Some("buster"));
    assert_eq!(fake.requests_for("server.config").len(), 1);
}

#[tokio::test]
async fn a_config_answer_without_the_lists_keeps_the_policy_unknown_until_it_has_them() {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", chips());
    let good = fixture("real_server_config.json")["config"]["authorization"].clone();
    printer.set_authorization(None);
    let (m, _) = spawn(&settings(&fake), fast(), false);
    until("a config read", || {
        fake.requests_for("server.config").len() >= 3
    })
    .await;
    assert!(!known(&m));
    assert_eq!(fake.connections(), 1);
    printer.set_authorization(Some(good));
    until("a known policy", || known(&m)).await;
    assert_eq!(fake.connections(), 1);
}

#[tokio::test]
async fn a_login_requirement_is_published_at_the_next_poll_and_lifted_at_the_one_after() {
    let (_fake, printer, m) = rig().await;
    until("a policy", || logins_required(&m) == Some(false)).await;
    printer.set_login_required(true);
    until("logins required", || logins_required(&m) == Some(true)).await;
    printer.set_login_required(false);
    until("logins not required", || logins_required(&m) == Some(false)).await;
}

#[tokio::test]
async fn the_hostname_arrives_once_printer_info_answers() {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", chips());
    printer.set_printer_info_fails(true);
    let (m, _) = spawn(&settings(&fake), fast(), false);
    until("a policy", || known(&m)).await;
    until("a few printer.info calls", || {
        fake.requests_for("printer.info").len() >= 3
    })
    .await;
    assert_eq!(hostname(&m), None);
    printer.set_printer_info_fails(false);
    until("a hostname", || hostname(&m).as_deref() == Some("buster")).await;
}

#[tokio::test]
async fn losing_the_connection_forgets_the_policy_with_the_control_handle() {
    let (fake, _printer, m) = rig().await;
    until("a policy", || known(&m)).await;
    fake.set_accepting(false);
    fake.close_connections();
    until("no control and no policy", || {
        m.control().is_none() && !known(&m)
    })
    .await;
    fake.set_accepting(true);
    until("a policy again", || known(&m)).await;
}

#[tokio::test]
async fn a_login_requirement_survives_a_lost_connection_until_a_read_says_otherwise() {
    let (fake, printer, m) = rig().await;
    until("a policy", || known(&m)).await;
    printer.set_login_required(true);
    until("logins required", || logins_required(&m) == Some(true)).await;

    fake.set_accepting(false);
    fake.close_connections();
    until("the marker", || {
        m.access()
            == AccessState::Unknown {
                logins_required: true,
            }
    })
    .await;
    printer.set_login_required(false);
    fake.set_accepting(true);
    until("logins lifted", || logins_required(&m) == Some(false)).await;
}

#[tokio::test]
async fn a_daemon_that_never_reached_moonraker_has_no_marker() {
    let fake = FakeMoonraker::start().await;
    FakePrinter::new(&fake, "ready", chips());
    fake.set_accepting(false);
    let (m, _) = spawn(&settings(&fake), fast(), false);
    until("an attempt", || fake.attempts().len() >= 2).await;
    assert_eq!(m.access(), AccessState::default());
}

#[tokio::test]
async fn an_unauthorized_config_read_is_the_unauthorized_status_with_no_policy() {
    let fake = FakeMoonraker::start().await;
    FakePrinter::new(&fake, "ready", chips());
    fake.on("server.config", Script::error(-32602, "Unauthorized"));
    let (m, _) = spawn(&settings(&fake), fast(), false);
    until("unauthorized", || {
        m.status().moonraker.state == MoonrakerState::Unauthorized
    })
    .await;
    assert!(!known(&m));
}

#[tokio::test]
async fn a_slow_chip_read_does_not_delay_a_login_change() {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", chips());
    let answer = json!({"eventtime": 1.0, "status": {"configfile": {"config": chips()}}});
    fake.on(
        "printer.objects.query",
        Script::after(ms(400), Script::Result(answer)),
    );
    let (m, _) = spawn(&settings(&fake), fast(), false);
    until("a policy", || known(&m)).await;
    printer.set_login_required(true);
    // The access task polls on its own, so the 400 ms chip read does not hold the change back.
    tokio::time::sleep(ms(150)).await;
    assert_eq!(logins_required(&m), Some(true));
}

#[tokio::test]
async fn polls_with_unchanged_answers_wake_nobody() {
    let (fake, _printer, m) = rig().await;
    until("a hostname", || hostname(&m).is_some()).await;
    let mut rx = m.watch_access();
    rx.mark_unchanged();
    let before = fake.requests_for("access.info").len();
    until("ten polls", || {
        fake.requests_for("access.info").len() >= before + 10
    })
    .await;
    assert!(!rx.has_changed().unwrap());
    assert_eq!(fake.requests_for("server.config").len(), 1);
}
