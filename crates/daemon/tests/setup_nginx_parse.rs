mod common;

use common::nginx_tree::Tree;
use wobblescope::setup::nginx::Layout;

fn labels(servers: &[wobblescope::setup::nginx::Server]) -> Vec<String> {
    servers.iter().map(|s| s.label()).collect()
}

#[test]
fn both_sites_of_a_two_site_layout_map_to_the_instance_behind_the_upstream() {
    let tree = Tree::new(&["fluidd", "mainsail"]);
    let layout = Layout::read(&tree.root()).unwrap();
    assert!(layout.skipped.is_empty(), "{:?}", layout.skipped);
    let plan = layout.plan(7125);
    assert_eq!(
        labels(&plan.mapped),
        ["fluidd (listen 80)", "mainsail (listen 81)"]
    );
    assert!(plan.left_alone.is_empty());
    assert!(!plan.mapped[0].is_mainsail());
    assert!(plan.mapped[1].is_mainsail());
    let first = plan.mapped[1].first_location.unwrap();
    let text = std::fs::read_to_string(tree.available("mainsail")).unwrap();
    assert!(text[first..].starts_with("location / {"));
}

#[test]
fn sites_that_proxy_another_instance_are_left_alone_with_a_reason() {
    let tree = Tree::new(&["fluidd", "mainsail"]);
    let layout = Layout::read(&tree.root()).unwrap();
    let plan = layout.plan(7126);
    assert!(plan.mapped.is_empty());
    assert_eq!(plan.left_alone.len(), 2);
    assert!(
        plan.left_alone[0].1.contains("port 7125, not 7126"),
        "{:?}",
        plan.left_alone
    );
}

#[test]
fn a_literal_address_and_an_upstream_can_point_at_different_instances() {
    let tree = Tree::new(&["fluidd"]);
    let second = common::nginx_tree::fixture("mainsail")
        .replace("http://apiserver", "http://127.0.0.1:7126");
    tree.site("mainsail", &second);
    let layout = Layout::read(&tree.root()).unwrap();
    assert_eq!(labels(&layout.plan(7125).mapped), ["fluidd (listen 80)"]);
    assert_eq!(labels(&layout.plan(7126).mapped), ["mainsail (listen 81)"]);
}

#[test]
fn a_file_it_cannot_understand_is_skipped_and_named() {
    let tree = Tree::new(&["fluidd"]);
    tree.site("broken", "server { listen 82;\n");
    let layout = Layout::read(&tree.root()).unwrap();
    assert_eq!(layout.skipped.len(), 1);
    assert!(layout.skipped[0].0.ends_with("broken"));
    assert!(layout.skipped[0].1.contains("not understood"));
    assert_eq!(layout.plan(7125).mapped.len(), 1);
}

#[test]
fn servers_without_a_moonraker_proxy_and_commented_ones_are_ignored() {
    let tree = Tree::new(&["fluidd"]);
    tree.site(
        "static",
        "server {\n listen 8000;\n root /srv;\n location / { try_files $uri =404; }\n}\n",
    );
    tree.site(
        "commented",
        "# server { location /websocket { proxy_pass http://apiserver/websocket; } }\n",
    );
    let layout = Layout::read(&tree.root()).unwrap();
    let plan = layout.plan(7125);
    assert_eq!(plan.mapped.len(), 1);
    assert!(plan.left_alone.is_empty());
}

#[test]
fn a_backup_in_a_globbed_directory_is_not_read_as_a_site() {
    let tree = Tree::new(&["fluidd"]);
    let backup = tree.root().join("sites-enabled/fluidd.before-wobblescope");
    std::fs::write(backup, common::nginx_tree::fixture("fluidd")).unwrap();
    let layout = Layout::read(&tree.root()).unwrap();
    assert_eq!(layout.plan(7125).mapped.len(), 1);
}

#[test]
fn no_nginx_conf_means_no_layout() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Layout::read(dir.path()).is_none());
}
