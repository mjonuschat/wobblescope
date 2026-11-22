//! A KIAUH-style `/etc/nginx` in a temporary directory, built from the fixtures in
//! `tests/data/nginx`.

use std::path::{Path, PathBuf};

pub struct Tree {
    pub dir: tempfile::TempDir,
}

pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/nginx")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

impl Tree {
    /// `nginx.conf`, `conf.d`, and `sites-enabled` symlinks into `sites-available` for each of
    /// `sites` (fixture names).
    pub fn new(sites: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for sub in ["conf.d", "sites-available", "sites-enabled"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        std::fs::write(root.join("nginx.conf"), fixture("nginx.conf")).unwrap();
        for name in ["upstreams.conf", "common_vars.conf"] {
            std::fs::write(root.join("conf.d").join(name), fixture(name)).unwrap();
        }
        let tree = Self { dir };
        for site in sites {
            tree.site(site, &fixture(site));
        }
        tree
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    /// Writes `sites-available/name` and links it from `sites-enabled`.
    pub fn site(&self, name: &str, text: &str) -> PathBuf {
        let target = self.root().join("sites-available").join(name);
        std::fs::write(&target, text).unwrap();
        let link = self.root().join("sites-enabled").join(name);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        target
    }

    pub fn available(&self, name: &str) -> PathBuf {
        self.root().join("sites-available").join(name)
    }
}
