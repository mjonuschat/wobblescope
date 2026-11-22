mod common;

use common::setup_rig::{CONF, Rig, mode_of};
use wobblescope::setup::Mode;

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
            .all(|c| c.starts_with("is-enabled") || c.starts_with("is-active") || c == "nginx -v"),
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
