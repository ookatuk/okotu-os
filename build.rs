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

    // 1. Git Hash
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    // 2. Directory Hash
    let dir_hash = calculate_dir_hash(src_dir.to_str().unwrap()).unwrap();

    // 3. Rust Version (Short)
    let rust_ver_full = Command::new("rustc")
        .args(["--version"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    let rust_ver = rust_ver_full
        .split('(')
        .next()
        .unwrap_or("unknown")
        .trim()
        .split(' ')
        .nth(1)
        .unwrap_or("unknown");
    let rust_ver_info = rust_ver_full
        .split('(')
        .nth(1) // "0c68443b0 2026-03-10)" が取れる
        .and_then(|s| s.split(')').next()) // ")" で分割して前を取る -> "0c68443b0 2026-03-10"
        .unwrap_or("unknown")
        .trim();

    // 4. Profile
    let mut profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    if profile == "debug" {
        profile = "dev".to_string();
    }

    // 5. Cycle
    let cycle = std::env::var("OS_CYCLE").unwrap_or_else(|_| "dev".into());
    if cycle != profile {
        println!(
            "cargo:warning=OS_CYCLE({}) does not match PROFILE({}).",
            cycle, profile
        );
    }

    // ブランチ名の取得
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // 開発者名（git config user.name）の取得
    let user = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let remote_url = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let opt = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());
    let debug = std::env::var("DEBUG").unwrap_or_else(|_| "false".to_string());
    let features: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
        .map(|(k, _)| k.replace("CARGO_FEATURE_", "").to_lowercase())
        .collect();

    println!("cargo:rustc-env=BUILD_OPT_LEVEL={}", opt);
    println!("cargo:rustc-env=BUILD_DEBUG={}", debug);
    println!("cargo:rustc-env=BUILD_FEATURES={}", features.join(","));
    println!("cargo:rustc-env=GIT_URL={}", remote_url);
    println!("cargo:rustc-env=GIT_BRANCH={}", branch);
    println!("cargo:rustc-env=GIT_USER={}", user);
    println!("cargo:rustc-env=DIR_HASH={}", dir_hash);
    println!("cargo:rustc-env=GIT_HASH={}", hash);
    println!("cargo:rustc-env=RUST_VER={}", rust_ver);
    println!("cargo:rustc-env=RUST_VERSION_INFO={}", rust_ver_info);
    println!("cargo:rustc-env=OS_PROFILE={}", profile);
    println!("cargo:rustc-env=OS_CYCLE={}", cycle);
}
