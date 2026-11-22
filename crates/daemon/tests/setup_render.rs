use std::path::Path;

use wobblescope::setup::render::*;

#[test]
fn the_unit_quotes_and_escapes_paths_and_orders_after_moonraker() {
    let text = unit(
        "pi",
        Path::new("/home/pi/my wobble%scope/wobblescope-aarch64"),
        Path::new("/home/pi/printer_data/config/wobblescope.toml"),
    );
    assert!(text.contains("User=pi\n"));
    assert!(text.contains("ExecStart=\"/home/pi/my wobble%%scope/wobblescope-aarch64\"\n"));
    assert!(text.contains(
        "Environment=\"WOBBLESCOPE_CONFIG=/home/pi/printer_data/config/wobblescope.toml\""
    ));
    assert!(text.contains("After=moonraker.service\nWants=moonraker.service\n"));
    assert!(text.contains("Restart=always\nRestartSec=5\n"));
    assert!(text.contains("WantedBy=multi-user.target\n"));
}

#[test]
fn the_service_list_edits_keep_order_and_ignore_repeats() {
    assert_eq!(
        with_service("a\nb", "wobblescope").unwrap(),
        "a\nb\nwobblescope\n"
    );
    assert_eq!(with_service("a\nwobblescope\n", "wobblescope"), None);
    assert_eq!(
        without_service("a\nwobblescope\nb\n", "wobblescope").unwrap(),
        "a\nb\n"
    );
    assert_eq!(without_service("a\n", "wobblescope"), None);
}

#[test]
fn the_config_text_and_mode_follow_the_key() {
    let (text, mode) = config_file(None, None);
    assert_eq!(mode, 0o644);
    assert!(!text.contains("moonraker_api_key"));
    let (text, mode) = config_file(Some("http://127.0.0.1:7140"), Some("k\"ey"));
    assert_eq!(mode, 0o600);
    let parsed: toml::Table = text.parse().unwrap();
    assert_eq!(parsed["moonraker_api_key"].as_str(), Some("k\"ey"));
    assert_eq!(
        parsed["moonraker_url"].as_str(),
        Some("http://127.0.0.1:7140")
    );
}

fn server_offsets(text: &str) -> (usize, usize, usize) {
    (
        text.find('{').unwrap(),
        text.rfind('}').unwrap(),
        text.find("location").unwrap(),
    )
}

#[test]
fn the_include_block_takes_the_indentation_of_the_first_location() {
    let text = "server {\n\tlisten 80;\n\n\tlocation / {\n\t}\n}\n";
    let (open, close, first) = server_offsets(text);
    let edited = with_include_block(text, open, close, first)
        .unwrap()
        .unwrap();
    assert_eq!(
        edited,
        "server {\n\tlisten 80;\n\n\t# BEGIN wobblescope\n\tinclude /etc/nginx/snippets/wobblescope.conf;\n\t# END wobblescope\n\tlocation / {\n\t}\n}\n"
    );
    let (open, close, first) = server_offsets(&edited);
    assert_eq!(with_include_block(&edited, open, close, first), Ok(None));
    assert_eq!(without_include_blocks(&edited).unwrap().unwrap(), text);
}

#[test]
fn an_edited_block_is_replaced_and_unbalanced_markers_are_refused() {
    let text = "server {\n  # BEGIN wobblescope\n  include /tmp/x;\n  # END wobblescope\n  location / {}\n}\n";
    let (open, close, first) = server_offsets(text);
    let fixed = with_include_block(text, open, close, first)
        .unwrap()
        .unwrap();
    assert!(fixed.contains("  include /etc/nginx/snippets/wobblescope.conf;\n"));
    assert!(!fixed.contains("/tmp/x"));
    let broken = "server {\n  # BEGIN wobblescope\n  location / {}\n}\n";
    let (open, close, first) = server_offsets(broken);
    assert!(with_include_block(broken, open, close, first).is_err());
}

#[test]
fn the_snippet_has_only_locations_and_the_daemon_port() {
    let text = snippet(7200);
    assert!(text.starts_with("# managed by wobblescope setup\n"));
    assert!(!text.contains("map ") && !text.contains("upstream"));
    assert_eq!(text.matches("location ").count(), 5);
    assert!(text.contains("location ^~ /wobblescope/ {"));
    assert!(text.contains("proxy_pass http://127.0.0.1:7200/;"));
    assert!(text.contains("client_max_body_size 64m;"));
}

#[test]
fn every_directive_in_the_snippet_is_indented_inside_its_location() {
    let mut inside = false;
    for line in snippet(7200).lines() {
        if line.starts_with("location ") {
            inside = !line.ends_with('}');
        } else if line == "}" {
            inside = false;
        } else if inside && !line.is_empty() {
            assert!(line.starts_with("    "), "not indented: {line:?}");
        }
    }
}

#[test]
fn trusted_proxies_is_added_extended_or_left_alone_and_the_rest_of_the_file_is_kept() {
    let added = with_trusted_proxies("# c\nlisten = \"0.0.0.0:7140\"\n")
        .unwrap()
        .unwrap();
    assert_eq!(
        added,
        "# c\nlisten = \"0.0.0.0:7140\"\ntrusted_proxies = [\"127.0.0.1\", \"::1\"]\n"
    );
    let tabled = with_trusted_proxies("a = 1\n[x]\nb = 2\n")
        .unwrap()
        .unwrap();
    assert_eq!(
        tabled,
        "a = 1\ntrusted_proxies = [\"127.0.0.1\", \"::1\"]\n\n[x]\nb = 2\n"
    );
    let extended = with_trusted_proxies(
        "trusted_proxies = [\n  \"10.0.0.0/8\", # lan\n  \"::1\",\n]\nb = 2\n",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        extended,
        "trusted_proxies = [\"10.0.0.0/8\", \"::1\", \"127.0.0.1\"]\nb = 2\n"
    );
    assert_eq!(
        with_trusted_proxies("trusted_proxies = [\"127.0.0.0/8\", \"::1\"]\n"),
        Ok(None)
    );
    assert!(with_trusted_proxies("trusted_proxies = 3\n").is_err());
    assert!(with_trusted_proxies("= broken").is_err());
}
