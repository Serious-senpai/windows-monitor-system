use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[allow(dead_code)]
struct CommonPaths {
    pub project_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub exe_dir: PathBuf,
    pub deploy_dir: PathBuf,
    pub cert_dir: PathBuf,
    pub out_dir: PathBuf,
}

impl CommonPaths {
    fn new() -> Self {
        let env_cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let env_profile = env::var("PROFILE").unwrap();
        let env_out_dir = env::var("OUT_DIR").unwrap();

        let project_dir = Path::new(&env_cargo_manifest_dir).to_path_buf();
        let workspace_dir = project_dir.parent().unwrap().to_path_buf();
        let exe_dir = workspace_dir.join("target").join(&env_profile);
        let deploy_dir = project_dir.join("deploy");
        let cert_dir = workspace_dir.join("cert");
        let out_dir = Path::new(&env_out_dir).to_path_buf();

        Self {
            project_dir,
            workspace_dir,
            exe_dir,
            deploy_dir,
            cert_dir,
            out_dir,
        }
    }
}

fn execute(cmd: &str, args: &[&str]) {
    let status = Command::new(cmd).args(args).status().unwrap();
    assert!(status.success());
}

fn create_client_certificate(paths: &CommonPaths) {
    println!(
        "cargo:rerun-if-changed={}",
        paths.cert_dir.join("server.pem").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        paths.cert_dir.join("server.rsa").display()
    );
    execute(
        "openssl",
        &[
            "req",
            "-new",
            "-newkey",
            "rsa:4096",
            "-sha512",
            "-nodes",
            "-keyout",
            &paths.out_dir.join("client.rsa").to_string_lossy(),
            "-out",
            &paths.out_dir.join("client.csr").to_string_lossy(),
            "-subj",
            "/CN=client",
        ],
    );
    execute(
        "openssl",
        &[
            "x509",
            "-req",
            "-days",
            "3650",
            "-sha512",
            "-in",
            &paths.out_dir.join("client.csr").to_string_lossy(),
            "-CA",
            &paths.cert_dir.join("server.pem").to_string_lossy(),
            "-CAkey",
            &paths.cert_dir.join("server.rsa").to_string_lossy(),
            "-CAcreateserial",
            "-out",
            &paths.out_dir.join("client.pem").to_string_lossy(),
        ],
    );

    execute(
        "openssl",
        &[
            "pkcs12",
            "-export",
            "-out",
            &paths.out_dir.join("client.pfx").to_string_lossy(),
            "-inkey",
            &paths.out_dir.join("client.rsa").to_string_lossy(),
            "-in",
            &paths.out_dir.join("client.pem").to_string_lossy(),
            "-passout",
            "env:WINDOWS_MONITOR_PASSWORD",
        ],
    );
}

fn main() {
    let paths = CommonPaths::new();

    create_client_certificate(&paths);
}
