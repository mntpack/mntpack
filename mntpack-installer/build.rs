use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    if let Err(err) = build_payload() {
        panic!("failed to build embedded mntpack payload: {err}");
    }
}

fn build_payload() -> Result<(), String> {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").map_err(|e| format!("CARGO_MANIFEST_DIR missing: {e}"))?,
    );
    let repo_root = manifest_dir
        .parent()
        .ok_or_else(|| "failed to resolve repository root".to_string())?;
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| format!("OUT_DIR missing: {e}"))?);
    let profile = env::var("PROFILE").map_err(|e| format!("PROFILE missing: {e}"))?;
    let target = env::var("TARGET").map_err(|e| format!("TARGET missing: {e}"))?;

    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("Cargo.lock").display()
    );
    println!("cargo:rerun-if-changed={}", repo_root.join("src").display());

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(repo_root.join("Cargo.toml"))
        .arg("--bin")
        .arg("mntpack")
        .arg("--target")
        .arg(&target)
        .env("CARGO_TARGET_DIR", out_dir.join("payload-target"));

    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to spawn cargo for mntpack payload: {e}"))?;
    if !status.success() {
        return Err(format!(
            "cargo build for mntpack payload failed with status {status}"
        ));
    }

    let source_binary = payload_binary_path(&out_dir, &target, &profile);
    if !source_binary.exists() {
        return Err(format!(
            "expected payload binary does not exist: {}",
            source_binary.display()
        ));
    }

    let destination = out_dir.join("mntpack_payload.bin");
    fs::copy(&source_binary, &destination).map_err(|e| {
        format!(
            "failed to copy payload binary {} -> {}: {e}",
            source_binary.display(),
            destination.display()
        )
    })?;

    Ok(())
}

fn payload_binary_path(out_dir: &Path, target: &str, profile: &str) -> PathBuf {
    let mut base = out_dir.join("payload-target").join(target).join(profile);
    if cfg!(windows) {
        base.push("mntpack.exe");
    } else {
        base.push("mntpack");
    }
    base
}
