use std::path::Path;
use std::{env, fs};

fn main() {
    let env_cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let env_profile = env::var("PROFILE").unwrap();

    let wm_client_dir = Path::new(&env_cargo_manifest_dir);
    let workspace_dir = wm_client_dir.parent().unwrap();

    let deploy_dir = wm_client_dir.join("deploy");
    let exe_dir = workspace_dir.join("target").join(&env_profile);

    fs::create_dir_all(&exe_dir).unwrap();
    for file in deploy_dir.read_dir().unwrap() {
        let source = file.unwrap().path();
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(&source, exe_dir.join(source.file_name().unwrap())).unwrap();
    }
}
