use std::process::Command;
use std::time::SystemTime;

#[cfg(feature = "web")]
use std::path::Path;

fn main() {
    // Build date (stdlib-only, no chrono dependency needed)
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();
    let days = secs / 86400;
    // Convert days since epoch to YYYY-MM-DD
    let (year, month, day) = days_to_date(days);
    let date = format!("{year:04}-{month:02}-{day:02}");
    println!("cargo:rustc-env=BUILD_DATE={date}");

    // Rust compiler version
    let rustc_version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().replace("rustc ", ""));
    // Extract just version number (e.g., "1.75.0" from "1.75.0 (82e1608df 2023-12-21)")
    let rustc_short = rustc_version.split_whitespace().next().unwrap_or("unknown");
    println!("cargo:rustc-env=RUSTC_VERSION={rustc_short}");

    // Rerun if build.rs changes
    println!("cargo:rerun-if-changed=build.rs");

    // Build web frontend when web feature is enabled
    #[cfg(feature = "web")]
    build_web_frontend();
}

#[cfg(feature = "web")]
fn build_web_frontend() {
    let web_dir = Path::new("web");
    let dist_dir = web_dir.join("dist");
    let src_dir = web_dir.join("src");

    // Rerun if config files change
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/package.json");
    println!("cargo:rerun-if-changed=web/tsconfig.json");
    println!("cargo:rerun-if-changed=web/tailwind.config.js");
    println!("cargo:rerun-if-changed=web/vite.config.ts");

    // Rerun if any source files change (enumerate files, not directory)
    if let Ok(files) = walkdir(&src_dir) {
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
        }
    }

    // Check if build is needed by comparing timestamps
    if !needs_rebuild(&src_dir, &dist_dir) {
        return;
    }

    println!("cargo:warning=Building web frontend...");

    // Try shell script first (Unix), fall back to direct npm commands (Windows/CI)
    let build_result = if cfg!(unix) {
        Command::new("bash").arg("web/build.sh").status()
    } else {
        // Windows: run npm commands directly
        let npm_cmd = if cfg!(windows) { "npm.cmd" } else { "npm" };

        // Install dependencies if needed
        if !web_dir.join("node_modules").exists() {
            let install = Command::new(npm_cmd)
                .args(["ci", "--silent"])
                .current_dir(web_dir)
                .status();

            if let Err(e) = install {
                println!(
                    "cargo:warning=Failed to install npm dependencies: {e}. Web UI may not work."
                );
                return;
            }
        }

        // Build frontend
        Command::new(npm_cmd)
            .args(["run", "build", "--silent"])
            .current_dir(web_dir)
            .status()
    };

    match build_result {
        Ok(status) if status.success() => {
            println!("cargo:warning=Web frontend build complete");
        }
        Ok(status) => {
            println!("cargo:warning=Web frontend build failed with status: {status}");
        }
        Err(e) => {
            // npm not found - provide helpful message
            if e.kind() == std::io::ErrorKind::NotFound {
                println!("cargo:warning=npm not found. Install Node.js to build web UI, or disable the 'web' feature.");
            } else {
                println!("cargo:warning=Failed to build web frontend: {e}");
            }
        }
    }
}

#[cfg(feature = "web")]
fn needs_rebuild(src_dir: &Path, dist_dir: &Path) -> bool {
    let dist_index = dist_dir.join("index.html");

    // If dist doesn't exist, need to build
    if !dist_index.exists() {
        return true;
    }

    // Get dist modification time
    let dist_mtime = match std::fs::metadata(&dist_index) {
        Ok(m) => match m.modified() {
            Ok(t) => t,
            Err(_) => return true,
        },
        Err(_) => return true,
    };

    // Check if any source file is newer than dist
    if let Ok(entries) = walkdir(src_dir) {
        for entry in entries {
            if let Ok(meta) = std::fs::metadata(&entry) {
                if let Ok(mtime) = meta.modified() {
                    if mtime > dist_mtime {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(feature = "web")]
fn walkdir(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path)?);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Convert days since Unix epoch to (year, month, day).
/// Uses the civil calendar algorithm from Howard Hinnant.
fn days_to_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
