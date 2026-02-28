use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let external_dir = PathBuf::from(&manifest_dir).join("log_viewer");

    let host_target = env::var("HOST").expect("HOST env var not found");
    let binary_name = "ookatuks_os_log_viewer";

    println!(
        "Build Script: Configuring {} for host {}",
        binary_name, host_target
    );

    // 2. Build for host OS
    let status = Command::new("cargo")
        .args(&["build", "--release", "--target", &host_target])
        .current_dir(&external_dir)
        .status()
        .expect("Failed to execute cargo build command");

    if !status.success() {
        panic!("External build failed with status: {}", status);
    }

    let exe_ext = if host_target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let binary_path = external_dir
        .join("target")
        .join(&host_target)
        .join("release")
        .join(format!("{}{}", binary_name, exe_ext));

    let dist_dir = PathBuf::from(&manifest_dir).join("bin");
    let dest_path = dist_dir.join(format!("log_viewer{}", exe_ext));

    std::fs::create_dir_all(&dist_dir).ok();

    if let Err(e) = std::fs::copy(&binary_path, &dest_path) {
        panic!(
            "Failed to copy binary.\nSource: {}\nDest: {}\nError: {}",
            binary_path.display(),
            dest_path.display(),
            e
        );
    }

    println!(
        "Build Script: Successfully copied binary to {}",
        dest_path.display()
    );

    println!("cargo:rerun-if-changed=log_viewer/src");
    println!("cargo:rerun-if-changed=log_viewer/Cargo.lock");
}
