mod common;

use std::os::unix::fs::{MetadataExt, PermissionsExt};

use common::nginx_tree::{Tree, fixture};
use common::setup_rig::{Rig, mode_of};
use wobblescope::setup::Mode;

const BLOCK: &str = "    # BEGIN wobblescope\n    include /etc/nginx/snippets/wobblescope.conf;\n    # END wobblescope\n";

async fn rig_with(sites: &[&str]) -> (Rig, Tree) {
    let mut rig = Rig::new().await;
    let tree = Tree::new(sites);
    tree.moonraker_port(rig.moonraker_port());
    rig.nginx_dir = tree.root();
    (rig, tree)
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[tokio::test]
async fn install_writes_the_snippet_one_include_per_site_the_backups_and_trusted_proxies() {
    let (rig, tree) = rig_with(&["fluidd", "mainsail"]).await;
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");

    let snippet = tree.root().join("snippets/wobblescope.conf");
    let text = read(&snippet);
    assert!(text.starts_with("# managed by wobblescope setup\n"));
    assert!(text.contains("proxy_pass http://127.0.0.1:7140/;"));
    assert!(text.contains("proxy_pass http://127.0.0.1:7140/api/live;"));
    assert_eq!(mode_of(&snippet), 0o644);
    assert!(
        !std::fs::symlink_metadata(&snippet)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    for name in ["fluidd", "mainsail"] {
        let site = read(&tree.available(name));
        assert_eq!(site.matches("# BEGIN wobblescope").count(), 1);
        let expected =
            fixture(name).replacen("    location / {", &format!("{BLOCK}    location / {{"), 1);
        assert_eq!(site, expected, "{name}");
        let backup = tree.available(&format!("{name}.before-wobblescope"));
        assert_eq!(read(&backup), fixture(name));
        assert!(
            std::fs::symlink_metadata(tree.root().join("sites-enabled").join(name))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    assert_eq!(rig.systemd.count("nginx -t"), 1);
    assert_eq!(rig.systemd.count("reload nginx"), 1);
    let config: toml::Table = read(&rig.config()).parse().unwrap();
    let proxies = config["trusted_proxies"].as_array().unwrap();
    assert_eq!(proxies.len(), 2);
    assert_eq!(read(&rig.conf()), common::setup_rig::CONF);
}

#[tokio::test]
async fn a_second_run_writes_nothing_and_does_not_reload_nginx() {
    let (rig, tree) = rig_with(&["fluidd", "mainsail"]).await;
    rig.install().await.0.unwrap();
    let before = read(&tree.available("mainsail"));
    let inode = std::fs::metadata(tree.available("mainsail")).unwrap().ino();
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(read(&tree.available("mainsail")), before);
    assert_eq!(
        std::fs::metadata(tree.available("mainsail")).unwrap().ino(),
        inode
    );
    assert_eq!(rig.systemd.count("nginx -t"), 1);
    assert_eq!(rig.systemd.count("reload nginx"), 1);
}

#[tokio::test]
async fn a_rejected_configuration_is_put_back_and_the_failure_names_what_nginx_printed() {
    let (rig, tree) = rig_with(&["fluidd", "mainsail"]).await;
    rig.systemd.0.lock().unwrap().nginx_test_error = Some(
        "nginx: [emerg] unknown directive \"x\" in /etc/nginx/sites-enabled/fluidd:3".to_owned(),
    );
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(out.contains("unknown directive"), "{out}");
    assert!(out.contains("put back"), "{out}");
    for name in ["fluidd", "mainsail"] {
        assert_eq!(read(&tree.available(name)), fixture(name));
        assert!(
            !tree
                .available(&format!("{name}.before-wobblescope"))
                .exists()
        );
    }
    assert!(!tree.root().join("snippets/wobblescope.conf").exists());
    assert_eq!(rig.systemd.count("reload nginx"), 0);
    assert_eq!(rig.systemd.count("restart wobblescope"), 1);
}

#[tokio::test]
async fn no_nginx_leaves_nginx_and_trusted_proxies_alone() {
    let (rig, tree) = rig_with(&["fluidd"]).await;
    let mut options = rig.options(Mode::Install, None);
    options.nginx = false;
    let moonraker = rig.moonraker();
    let mut out = Vec::new();
    wobblescope::setup::execute(&rig.host(), &moonraker, &options, &mut out)
        .await
        .unwrap();
    assert!(String::from_utf8(out).unwrap().contains("--no-nginx"));
    assert_eq!(read(&tree.available("fluidd")), fixture("fluidd"));
    assert!(!tree.root().join("snippets").exists());
    assert!(!read(&rig.config()).contains("trusted_proxies"));
}

#[tokio::test]
async fn without_nginx_or_a_matching_site_setup_says_so_and_carries_on() {
    let rig = Rig::new().await;
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert!(out.contains("nginx") && out.contains("skipped"), "{out}");
    assert!(!read(&rig.config()).contains("trusted_proxies"));

    let (mut rig, tree) = rig_with(&["fluidd"]).await;
    tree.moonraker_port(1);
    rig.nginx_dir = tree.root();
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert!(out.contains("port 1, not"), "{out}");
    assert!(out.contains("no site proxies Moonraker"), "{out}");
    assert_eq!(read(&tree.available("fluidd")), fixture("fluidd"));
    assert!(!read(&rig.config()).contains("trusted_proxies"));
}

#[tokio::test]
async fn a_user_writable_symlink_at_the_snippet_path_is_replaced_not_written_through() {
    let (rig, tree) = rig_with(&["fluidd"]).await;
    let elsewhere = rig.base().join("user-file");
    std::fs::write(&elsewhere, "mine\n").unwrap();
    std::fs::create_dir_all(tree.root().join("snippets")).unwrap();
    let snippet = tree.root().join("snippets/wobblescope.conf");
    std::os::unix::fs::symlink(&elsewhere, &snippet).unwrap();
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(read(&elsewhere), "mine\n");
    assert!(
        !std::fs::symlink_metadata(&snippet)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(read(&snippet).starts_with("# managed by wobblescope setup"));
}

#[tokio::test]
async fn the_daemon_port_and_existing_proxies_come_from_the_config_file() {
    let (rig, _tree) = rig_with(&["fluidd"]).await;
    std::fs::write(
        rig.config(),
        format!(
            "# mine\nmoonraker_url = \"{}\"\nlisten = \"0.0.0.0:7200\"\ntrusted_proxies = [\"192.168.0.0/16\"] # lan\nretention_cap_mib = 10\n",
            rig.fake.http_url()
        ),
    )
    .unwrap();
    let (result, out) = rig.install().await;
    assert_eq!(result, Ok(true), "{out}");
    let text = read(&rig.config());
    assert!(text.starts_with("# mine\n"));
    assert!(text.contains("retention_cap_mib = 10\n"));
    assert!(
        text.contains("[\"192.168.0.0/16\", \"127.0.0.1\", \"::1\"]"),
        "{text}"
    );
    let snippet = read(&rig.nginx_dir.join("snippets/wobblescope.conf"));
    assert!(snippet.contains("http://127.0.0.1:7200/;"));
}

#[tokio::test]
async fn a_site_file_behind_a_symlink_keeps_its_mode_and_stays_a_symlink() {
    let (rig, tree) = rig_with(&["fluidd"]).await;
    std::fs::set_permissions(
        tree.available("fluidd"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    rig.install().await.0.unwrap();
    assert_eq!(mode_of(&tree.available("fluidd")), 0o640);
    assert!(
        std::fs::symlink_metadata(tree.root().join("sites-enabled/fluidd"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(mode_of(&tree.available("fluidd.before-wobblescope")), 0o640);
}

#[tokio::test]
async fn check_reports_each_piece_and_changes_nothing() {
    let (rig, tree) = rig_with(&["fluidd", "mainsail"]).await;
    rig.install().await.0.unwrap();
    let (result, out) = rig.run(Mode::Check, None).await;
    assert_eq!(result, Ok(true), "{out}");
    for label in ["nginx snippet", "nginx site", "nginx -t", "trusted_proxies"] {
        assert!(
            out.lines().any(|l| l.contains(label) && l.contains("ok")),
            "{label}: {out}"
        );
    }

    let site = read(&tree.available("mainsail"));
    std::fs::write(tree.available("mainsail"), fixture("mainsail")).unwrap();
    std::fs::write(tree.root().join("snippets/wobblescope.conf"), "# other\n").unwrap();
    let (result, out) = rig.run(Mode::Check, None).await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(
        out.contains("mainsail (listen 81): include missing"),
        "{out}"
    );
    assert!(
        out.contains("nginx snippet") && out.contains("stale"),
        "{out}"
    );
    assert_eq!(read(&tree.available("mainsail")), fixture("mainsail"));
    assert_ne!(site, fixture("mainsail"));
    assert_eq!(rig.systemd.count("reload nginx"), 1);
}

#[tokio::test]
async fn check_flags_missing_trusted_proxies() {
    let (rig, _tree) = rig_with(&["fluidd"]).await;
    rig.install().await.0.unwrap();
    std::fs::write(
        rig.config(),
        format!("moonraker_url = \"{}\"\n", rig.fake.http_url()),
    )
    .unwrap();
    let (result, out) = rig.run(Mode::Check, None).await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(out.contains("missing 127.0.0.1 or ::1"), "{out}");
}

#[tokio::test]
async fn remove_takes_the_block_and_the_snippet_out_and_keeps_the_config() {
    let (rig, tree) = rig_with(&["fluidd", "mainsail"]).await;
    rig.install().await.0.unwrap();
    let config = read(&rig.config());
    let (result, out) = rig.run(Mode::Remove, None).await;
    assert_eq!(result, Ok(true), "{out}");
    for name in ["fluidd", "mainsail"] {
        assert_eq!(read(&tree.available(name)), fixture(name));
    }
    assert!(!tree.root().join("snippets/wobblescope.conf").exists());
    assert_eq!(read(&rig.config()), config);
    assert_eq!(rig.systemd.count("nginx -t"), 2);
    assert_eq!(rig.systemd.count("reload nginx"), 2);
    let (result, out) = rig.run(Mode::Remove, None).await;
    assert_eq!(result, Ok(true), "{out}");
    assert_eq!(rig.systemd.count("reload nginx"), 2);
}

#[tokio::test]
async fn remove_keeps_a_snippet_without_the_header_and_puts_back_what_nginx_rejects() {
    let (rig, tree) = rig_with(&["fluidd"]).await;
    rig.install().await.0.unwrap();
    let snippet = tree.root().join("snippets/wobblescope.conf");
    let installed = read(&tree.available("fluidd"));
    std::fs::write(&snippet, "location /x {}\n").unwrap();
    let (result, out) = rig.run(Mode::Remove, None).await;
    assert_eq!(result, Ok(true), "{out}");
    assert!(snippet.exists());
    assert_eq!(read(&tree.available("fluidd")), fixture("fluidd"));

    std::fs::write(tree.available("fluidd"), &installed).unwrap();
    std::fs::write(&snippet, wobblescope::setup::render::snippet(7140)).unwrap();
    rig.systemd.0.lock().unwrap().nginx_test_error = Some("nginx: [emerg] boom".to_owned());
    let (result, out) = rig.run(Mode::Remove, None).await;
    assert_eq!(result, Ok(false), "{out}");
    assert!(out.contains("boom"), "{out}");
    assert_eq!(read(&tree.available("fluidd")), installed);
    assert!(snippet.exists());
}
