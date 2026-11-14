// Sets APP_VERSION, the version the programs show (bottom left in the window,
// in settings, in the server's answer and in the procedure documentation).
//
// - Release workflow: OPENKONTOR_VERSION comes from the workflow (the version
//   typed in when starting it by hand, or the tag such as v1.2.3).
// - Any other GitHub Actions run: the Cargo version plus the run number,
//   e.g. 0.1.0-build.42.
// - Local builds: the Cargo version.
fn main() {
    println!("cargo:rerun-if-env-changed=OPENKONTOR_VERSION");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_NUMBER");

    let cargo = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let given = std::env::var("OPENKONTOR_VERSION")
        .ok()
        .map(|v| v.trim().trim_start_matches('v').to_string())
        .filter(|v| !v.is_empty());
    let version = match given {
        Some(v) => v,
        None => match (std::env::var("GITHUB_ACTIONS").as_deref(), std::env::var("GITHUB_RUN_NUMBER")) {
            (Ok("true"), Ok(run)) if !run.trim().is_empty() => format!("{cargo}-build.{}", run.trim()),
            _ => cargo,
        },
    };
    println!("cargo:rustc-env=APP_VERSION={version}");
}
