fn main() {
    let dist = std::path::Path::new("../../web/dist");
    if !dist.exists() {
        std::fs::create_dir_all(dist).expect("create web/dist");
    }
    println!("cargo:rerun-if-changed=../../web/dist");
}
