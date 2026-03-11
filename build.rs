use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

fn calculate_dir_hash(dir_path: &str) -> io::Result<String> {
    let mut hasher = Sha256::new();

    let mut entries: Vec<_> = WalkDir::new(dir_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    entries.sort_unstable_by(|a, b| a.path().cmp(b.path()));

    // 2. 各ファイルの内容をハッシュ計算機に流し込む
    for entry in entries {
        let mut file = File::open(entry.path())?;
        let mut buffer = [0; 8192];

        let path = entry.path();
        let relative_path = path.strip_prefix(dir_path).unwrap();
        hasher.update(relative_path.to_string_lossy().as_bytes());

        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
    }

    let result = hasher.finalize();
    let full_hash = hex::encode(result);

    let short_hash = &full_hash[..7];

    Ok(short_hash.to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=log_viewer/src");
    println!("cargo:rerun-if-changed=log_viewer/Cargo.lock");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=src");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let external_dir = PathBuf::from(&manifest_dir).join("log_viewer");

    let src_dir = PathBuf::from(&manifest_dir).join("src");

    let host_target = env::var("HOST").expect("HOST env var not found");
    let binary_name = "ookatuks_os_log_viewer";

    println!(
        "Build Script: Configuring {} for host {}",
        binary_name, host_target
    );

    let build_temp_dir = env::temp_dir().join("ookatuks_os_log_viewer_build");

    let status = Command::new("cargo")
        .env("RUSTFLAGS", "")
        .args(&[
            "build",
            "--release",
            "--target",
            &host_target,
            "--manifest-path",
            external_dir.join("Cargo.toml").to_str().unwrap(),
            "--target-dir",
            build_temp_dir.to_str().unwrap(),
        ])
        .current_dir("/")
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

    let binary_path = build_temp_dir
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

    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    let dir_hash = calculate_dir_hash(src_dir.to_str().unwrap()).unwrap();

    let rust_ver = Command::new("rustc")
        .args(["--version"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=OS_BUILD={}.{}", dir_hash, hash);
    println!("cargo:rustc-env=RUST_VER={}", rust_ver.trim());

    let mut profile = std::env::var("PROFILE").unwrap();

    if profile == "debug" {
        profile = "dev".to_string()
    }

    println!("cargo:rustc-env=OS_PROFILE={}", profile.trim());

    let cycle = std::env::var("OS_CYCLE").unwrap_or_else(|_| "dev".into());
    if cycle != profile {
        println!("cargo:warning=profile not match.");
    }
}
