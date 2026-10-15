fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-env-changed=SORORAIL_GIT_COMMIT");
    let commit = std::env::var("SORORAIL_GIT_COMMIT").unwrap_or_else(|_| {
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_else(|| "unknown".to_owned())
    });
    println!("cargo:rustc-env=SORORAIL_GIT_COMMIT={commit}");
}
