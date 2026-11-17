//! The Worker build tool pin, held in place by tests.
//!
//! `worker-build` compiles this crate against the `worker` crate in `Cargo.lock`. A `cargo
//! install worker-build` with no `@<version>` installs the newest release, which refuses an
//! older `worker` crate: 0.9.0 rejects `worker@0.8.7` with "Unsupported version worker@0.8.7,
//! expected at least worker@0.9.0". The CI step and `wrangler.jsonc`'s build command therefore
//! must both name the same exact release, and it must share major.minor with the locked
//! `worker` crate.

const WRANGLER_JSONC: &str = include_str!("../wrangler.jsonc");
const CI_YML: &str = include_str!("../../../.github/workflows/ci.yml");
const CARGO_LOCK: &str = include_str!("../Cargo.lock");

/// The version each `cargo install` of worker-build in `text` names, in order.
///
/// `None` is an install that names worker-build with no `@<version>`, the fault the pin exists
/// to prevent. Commands are split at whitespace, `&&`, `||`, `&` and `;`; the words are taken
/// with JSON punctuation removed, so a `build.command` inside `wrangler.jsonc` reads the same
/// way as a CI `run:` line.
fn installed_build_tool_versions(text: &str) -> Vec<Option<String>> {
    let mut versions = Vec::new();
    for command in text.split(['\n', '&', '|', ';']) {
        let words: Vec<&str> = command
            .split(|c: char| c.is_whitespace() || c == '"' || c == ',' || c == ':')
            .filter(|w| !w.is_empty())
            .collect();
        if !words.contains(&"install") {
            continue;
        }
        let Some(tool) = words.iter().find(|w| w.starts_with("worker-build")) else {
            continue;
        };
        versions.push(
            tool.strip_prefix("worker-build@")
                .filter(|v| !v.is_empty())
                .map(str::to_string),
        );
    }
    versions
}

/// The `version` of one package in a lock file, as Cargo writes it (`name =` then `version =`).
fn locked_package_version(lock: &str, package: &str) -> Option<String> {
    let name = format!("name = \"{package}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == name {
            let version = lines.next()?.trim();
            return version
                .strip_prefix("version = \"")?
                .strip_suffix('"')
                .map(str::to_string);
        }
    }
    None
}

/// The `major.minor` of a `major.minor.patch` version: the part the build tool and its `worker`
/// crate must agree on.
fn major_minor(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Both build commands install exactly the pinned release, and neither installs without one.
#[test]
fn build_tool_is_pinned_in_wrangler_and_ci() {
    // 0.8.7 is the release published 2026-09-25, the one installed on the deploy host, and the
    // one whose `worker` 0.8.7 is locked in this crate.
    let pin = "0.8.7";
    let wrangler = installed_build_tool_versions(WRANGLER_JSONC);
    let ci = installed_build_tool_versions(CI_YML);

    assert_eq!(
        wrangler,
        [Some(pin.to_string())],
        "crates/worker/wrangler.jsonc's build.command must install worker-build@{pin} exactly \
         once and never without a version, got {wrangler:?}"
    );
    assert_eq!(
        ci,
        [Some(pin.to_string())],
        ".github/workflows/ci.yml's `worker build` step must install worker-build@{pin} exactly \
         once and never without a version, got {ci:?}"
    );
    assert_eq!(
        wrangler, ci,
        "the build command and the CI step must name the same worker-build version"
    );
}

/// The pinned build tool shares major.minor with the `worker` crate this crate locks.
#[test]
fn build_tool_pin_matches_locked_worker() {
    let locked = locked_package_version(CARGO_LOCK, "worker")
        .expect("crates/worker/Cargo.lock must lock the `worker` package");
    let mut pinned: Vec<(&str, String)> = Vec::new();
    for (file, text) in [
        ("crates/worker/wrangler.jsonc", WRANGLER_JSONC),
        (".github/workflows/ci.yml", CI_YML),
    ] {
        for version in installed_build_tool_versions(text).into_iter().flatten() {
            pinned.push((file, version));
        }
    }

    assert!(
        !pinned.is_empty(),
        "the pinned worker-build version could not be read from the two build commands; the \
         version check needs one (build_tool_is_pinned_in_wrangler_and_ci reports why)"
    );
    for (file, version) in &pinned {
        assert_eq!(
            major_minor(version),
            major_minor(&locked),
            "{file} installs worker-build@{version}, which does not share major.minor with the \
             locked `worker` {locked}: worker-build refuses that crate version"
        );
    }
}
