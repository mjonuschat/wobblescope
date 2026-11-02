use std::net::IpAddr;

use serde_json::{Value, json};
use wobblescope::access::policy::parse_login_required;
use wobblescope::access::{IgnoredList, Policy};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/data/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn config(trusted: &Value, cors: &Value) -> Value {
    json!({"config": {"authorization": {"trusted_clients": trusted, "cors_domains": cors}}})
}

fn real() -> Policy {
    Policy::build(&fixture("real_server_config.json"), false, Some("buster")).unwrap()
}

fn with(trusted: Value, cors: Value) -> Policy {
    Policy::build(&config(&trusted, &cors), false, None).unwrap()
}

fn trusting(entries: Value) -> Policy {
    with(entries, json!([]))
}

fn allowing(cors: Value) -> Policy {
    with(json!([]), cors)
}

#[test]
fn the_real_lists_trust_the_private_ranges_and_nothing_else() {
    let p = real();
    for yes in [
        "192.168.1.20",
        "10.1.2.3",
        "172.16.0.9",
        "127.0.0.1",
        "::1",
        "fe80::1",
    ] {
        assert!(p.is_trusted(ip(yes)), "{yes}");
    }
    for no in ["203.0.113.9", "2001:db8::1"] {
        assert!(!p.is_trusted(ip(no)), "{no}");
    }
    assert!(p.ignored().is_empty(), "{:?}", p.ignored());
}

#[test]
fn a_mapped_client_is_compared_as_ipv4() {
    let p = real();
    assert!(p.is_trusted(ip("::ffff:192.168.1.20")));
    assert!(!p.is_trusted(ip("::ffff:203.0.113.9")));
}

#[test]
fn a_list_entry_in_mapped_form_keeps_its_family_and_matches_no_client() {
    let p = trusting(json!(["::ffff:10.0.0.0/104"]));
    assert!(!p.is_trusted(ip("10.0.0.5")));
    assert!(!p.is_trusted(ip("::ffff:10.0.0.5")));
}

#[test]
fn a_single_address_entry_trusts_only_that_address() {
    let p = trusting(json!(["192.168.5.7"]));
    assert!(p.is_trusted(ip("192.168.5.7")));
    assert!(!p.is_trusted(ip("192.168.5.8")));
}

#[test]
fn a_network_with_host_bits_set_is_dropped_and_listed() {
    let p = trusting(json!(["192.168.1.1/24"]));
    assert!(!p.is_trusted(ip("192.168.1.5")));
    assert_eq!(p.ignored().len(), 1);
    assert_eq!(p.ignored()[0].list, IgnoredList::TrustedClients);
    assert_eq!(p.ignored()[0].entry, "192.168.1.1/24");
}

#[test]
fn a_domain_entry_is_kept_for_lookups_and_trusts_no_address_by_itself() {
    let p = trusting(json!(["printer-host.lan", "not a host!"]));
    assert!(!p.is_trusted(ip("192.168.1.5")));
    assert!(p.has_domains());
    assert_eq!(p.trusted_domains(), ["printer-host.lan"]);
    assert!(p.is_trusted_name("PRINTER-HOST.LAN"));
    assert!(!p.is_trusted_name("host.lan"));
    assert!(!p.is_trusted_name("x.printer-host.lan"));
    assert_eq!(p.ignored().len(), 1);
    assert!(p.ignored()[0].reason.contains("neither"));
}

#[test]
fn an_empty_trusted_list_is_readable_and_trusts_nobody() {
    let p = trusting(json!([]));
    assert!(!p.is_trusted(ip("127.0.0.1")));
}

#[test]
fn an_answer_without_the_lists_is_unreadable_and_names_the_key() {
    let reason = |v: Value| Policy::build(&v, false, None).unwrap_err().to_string();
    assert!(reason(json!({"config": {}})).contains("authorization"));
    assert!(
        reason(config(&json!("10.0.0.0/8"), &json!([]))).contains("authorization.trusted_clients")
    );
    let no_cors = json!({"config": {"authorization": {"trusted_clients": []}}});
    assert!(reason(no_cors).contains("authorization.cors_domains"));
    assert!(reason(json!({})).contains("authorization"));
}

#[test]
fn a_wildcard_pattern_does_not_cover_a_port() {
    let p = allowing(json!(["*.local"]));
    assert!(p.origin_allowed("http://printer.local"));
    assert!(!p.origin_allowed("http://printer.local:7126"));
}

#[test]
fn a_pattern_must_cover_the_whole_origin() {
    let p = allowing(json!(["*://my.mainsail.xyz"]));
    assert!(p.origin_allowed("https://my.mainsail.xyz"));
    assert!(!p.origin_allowed("https://my.mainsail.xyz.evil.example"));
    assert!(!p.origin_allowed("https://evilmy.mainsail.xyz"));
}

#[test]
fn wildcards_in_the_top_level_domain_and_trailing_slashes_are_dropped() {
    let p = allowing(json!(["*.loc*", "http://printer.local/"]));
    assert!(!p.origin_allowed("http://a.local"));
    assert!(!p.origin_allowed("http://printer.local/"));
    assert_eq!(p.ignored().len(), 2);
    assert!(
        p.ignored()
            .iter()
            .all(|i| i.list == IgnoredList::CorsDomains)
    );
}

#[test]
fn the_first_matching_alternative_decides_as_in_python() {
    let p = allowing(json!(["http://a|http://ab"]));
    assert!(!p.origin_allowed("http://ab"));
    assert!(p.origin_allowed("http://a"));
}

#[test]
fn a_pattern_the_regex_engine_cannot_compile_admits_nothing() {
    let p = allowing(json!(["http://(?=a)a.local"]));
    assert!(!p.origin_allowed("http://a.local"));
    assert_eq!(p.ignored().len(), 1);
}

#[test]
fn without_an_accepted_pattern_even_a_trusted_ip_origin_is_refused() {
    let trusted = json!(["192.168.0.0/16"]);
    for cors in [json!([]), json!(["*.loc*"])] {
        let p = with(trusted.clone(), cors);
        assert!(!p.origin_allowed("http://192.168.1.20:7126"));
    }
}

#[test]
fn the_trusted_ip_clause_reads_the_host_as_moonraker_does() {
    let p = with(json!(["192.168.0.0/16", "::1/128"]), json!(["*.local"]));
    assert!(p.origin_allowed("http://192.168.1.20:7126"));
    assert!(!p.origin_allowed("http://203.0.113.9"));
    assert!(!p.origin_allowed("http://[::1]:7126"));
}

#[test]
fn host_parts_of_the_real_patterns_allow_their_names_case_insensitively() {
    let p = real();
    for yes in [
        "printer.local",
        "x.lan",
        "app.fluidd.xyz",
        "buster",
        "buster.local",
    ] {
        assert!(p.host_allowed(yes), "{yes}");
    }
    for no in [
        "rebind.example",
        "local",
        "fluidd.xyz",
        "my.mainsail.xyz.evil",
    ] {
        assert!(!p.host_allowed(no), "{no}");
    }
}

#[test]
fn wildcard_host_parts_add_nothing_and_are_listed() {
    for cors in [json!(["*"]), json!(["*://*"]), json!(["*://*mainsail.xyz"])] {
        let p = allowing(cors);
        assert!(!p.host_allowed("rebind.example"));
        assert!(!p.host_allowed("evilmainsail.xyz"));
        assert!(p.ignored().iter().any(|i| i.list == IgnoredList::Hosts));
    }
    assert!(allowing(json!(["*"])).origin_allowed("http://anything.example"));
}

#[test]
fn a_login_requirement_in_access_info_is_reported_by_the_policy() {
    let mut info = fixture("real_access_info.json");
    assert_eq!(parse_login_required(&info), Some(false));
    info["login_required"] = json!(true);
    let required = parse_login_required(&info).unwrap();
    let p = Policy::build(&fixture("real_server_config.json"), required, None).unwrap();
    assert!(p.logins_required());
    assert_eq!(parse_login_required(&json!({})), None);
}

#[test]
fn policies_built_from_the_same_answers_are_equal() {
    assert_eq!(real(), real());
    assert_ne!(real(), real().with_logins_required(true));
}

#[test]
fn ipv4_networks_may_be_written_with_a_netmask_or_a_hostmask() {
    let p = trusting(json!([
        "192.168.1.0/255.255.255.0",
        "10.0.0.0/0.255.255.255"
    ]));
    assert!(p.ignored().is_empty(), "{:?}", p.ignored());
    assert!(p.is_trusted(ip("192.168.1.20")));
    assert!(!p.is_trusted(ip("192.168.2.20")));
    assert!(p.is_trusted(ip("10.9.8.7")));
    assert!(!p.is_trusted(ip("11.0.0.1")));
}

#[test]
fn a_mask_that_is_neither_contiguous_nor_a_hostmask_is_invalid() {
    let p = trusting(json!(["192.168.1.0/255.0.255.0", "fe80::/ffff::"]));
    assert_eq!(p.ignored().len(), 2);
}
