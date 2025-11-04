use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Path to build number file
    let build_number_path = Path::new(".build_number");

    // Read current build number, or start at 1 if file doesn't exist
    let build_number: u32 = if build_number_path.exists() {
        fs::read_to_string(build_number_path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1)
    } else {
        1
    };

    // Increment build number
    let new_build_number = build_number + 1;

    // Write incremented build number back to file
    fs::write(build_number_path, new_build_number.to_string())
        .expect("Failed to write .build_number file");

    // Get base version from Cargo.toml (set at compile time)
    let cargo_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

    // Create full version string: e.g., "0.1.4+1234"
    let full_version = format!("{}+{}", cargo_version, new_build_number);

    // Set environment variables for use in the code
    println!("cargo:rustc-env=BUILD_NUMBER={}", new_build_number);
    println!("cargo:rustc-env=FULL_VERSION={}", full_version);

    // Rerun if Cargo.toml changes (version update)
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.build_number");
}
