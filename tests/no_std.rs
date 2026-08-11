//! What this file can and cannot prove.
//!
//! It **cannot** prove that the crate is `no_std`. Every test here runs inside a test
//! binary that links `std`, so `std` is present in the process no matter what the crate
//! under test declares. A passing run is compatible with the crate having quietly grown a
//! `std` dependency. `tests/public_api.rs` is in the same position and one step worse: the
//! `cronexpr` dev-dependency enables `jiff/default` in that graph, so its `tz-static` test
//! runs with exactly the `std` and `alloc` that tier promises to avoid.
//!
//! The only thing that proves `no_std` is a build for a target that has no `std` at all,
//! one per tier that claims to need no host:
//!
//! ```text
//! cargo check --lib --no-default-features --features tz-static --target thumbv7em-none-eabi
//! ```
//!
//! Those builds are the gate. They run in CI as the `no-std` job in
//! `.github/workflows/ci.yml`; if that job is ever removed, this crate has no `no_std`
//! guarantee left regardless of whether these tests pass.
//!
//! What the tests here do is narrower and still worth having. They hold the workflow to
//! having a cell for every tier that claims to need no host — a tier without one is a
//! claim without a gate, which is how `tz-static` shipped documented as bare-metal and
//! never built for a bare-metal target — and they pin the half of the no-alloc claim that
//! no target build can reach.

use std::path::{Path, PathBuf};

/// The bare-metal targets the CI gate builds for.
///
/// Kept here so that a grep for either target name from the test or the workflow finds the
/// other.
const BARE_METAL_TARGETS: &[&str] = &["thumbv6m-none-eabi", "thumbv7em-none-eabi"];

/// Whether a feature's tier claims to run with no operating system under it.
enum Host {
  /// The tier claims to need none, so it owes a cell in the `no-std` job.
  NotNeeded,
  /// The tier is a host tier by definition, and a bare-metal cell for it would state
  /// something other than what the tier promises.
  Required,
}

/// Every feature this crate declares, and which of the two it is.
///
/// The list is what makes the next unguarded tier impossible to add quietly. A feature
/// added to or removed from `Cargo.toml` changes the set
/// [`every_declared_feature_says_whether_it_needs_a_host`] reads and fails there until it
/// is classified here; a feature classified [`Host::NotNeeded`] fails
/// [`every_hostless_feature_has_a_bare_metal_cell`] until the workflow builds it for one.
///
/// `std` is `Required` even though `--features std` happens to build for a bare-metal
/// target today: it reaches no code of this crate's, so that build says nothing about the
/// tier and a cell would pin the accident instead of the claim.
const FEATURES: &[(&str, Host)] = &[
  ("default", Host::NotNeeded),
  ("alloc", Host::NotNeeded),
  ("std", Host::Required),
  ("jiff", Host::NotNeeded),
  ("tz-static", Host::NotNeeded),
  ("tz", Host::Required),
];

#[test]
fn bare_metal_gate_is_named_in_ci() {
  let workflow = include_str!("../.github/workflows/ci.yml");
  for target in BARE_METAL_TARGETS {
    assert!(
      workflow.contains(target),
      "a bare-metal build is the only proof of no_std and {target} no longer appears in \
       ci.yml"
    );
  }
}

#[test]
fn every_declared_feature_says_whether_it_needs_a_host() {
  let mut declared = declared_features(include_str!("../Cargo.toml"));
  declared.sort_unstable();
  let mut classified: Vec<&str> = FEATURES.iter().map(|(name, _)| *name).collect();
  classified.sort_unstable();

  assert_eq!(
    declared, classified,
    "the features in Cargo.toml and the ones classified in this file have diverged. A new \
     feature is a new tier claim: say here whether it needs a host, and if it does not, \
     give it a cell in the `no-std` job of ci.yml"
  );
}

#[test]
fn every_hostless_feature_has_a_bare_metal_cell() {
  let job = no_std_job(include_str!("../.github/workflows/ci.yml"));
  let built = features_built_in(&job);

  for (feature, host) in FEATURES {
    if matches!(host, Host::Required) {
      continue;
    }
    if *feature == "default" {
      // `default = []`, so this tier's cell is the one that passes no feature at all
      // rather than one `features_built_in` can see. Reading the matrix value directly
      // couples this to how the cell is written, which is the point: if the cell is
      // reformatted away, that is the same event as it being deleted.
      assert!(
        job.contains("features: ''"),
        "the default tier is the crate's primary configuration and the `no-std` job no \
         longer has a cell that builds it with no features"
      );
      continue;
    }
    assert!(
      built.iter().any(|name| name == feature),
      "`{feature}` claims to need no host and no cell of the `no-std` job builds it for a \
       bare-metal target. The tier's own tests cannot stand in: they run on a host, in a \
       graph that supplies the std and alloc the tier disclaims"
    );
  }
}

/// The no-alloc half of the claim, which is the half no target build reaches.
///
/// `alloc` is not in the extern prelude — an edition-2018-or-later crate can only name it
/// after an `extern crate alloc;` — so the absence of that item is not a proxy for this
/// crate's half of the no-alloc promise, it is the whole of it. A bare-metal `cargo check`
/// cannot say the same thing: `alloc` is in those targets' sysroot and an rlib links no
/// allocator, so a crate that grew an allocation would still build there.
///
/// What this cannot speak for is a dependency. That `jiff/static` needs no allocator is
/// jiff's promise to keep, and the `tz-static` row of the README passes it on.
#[test]
fn nothing_outside_the_alloc_feature_reaches_alloc() {
  let mut sources = Vec::new();
  rust_sources(
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
    &mut sources,
  );
  assert!(
    sources.iter().any(|path| path.ends_with("lib.rs")),
    "the walk of src/ found no lib.rs, so it is reading the wrong directory and every \
     assertion below it is vacuous"
  );

  for path in &sources {
    let source = std::fs::read_to_string(path).expect("a source file this crate compiles");
    let mut previous = "";
    for line in source.lines() {
      if line.trim_start().starts_with("extern crate alloc") {
        assert!(
          line.contains("feature = \"alloc\"") || previous.contains("feature = \"alloc\""),
          "{} reaches `alloc` without the `alloc` feature. The crate's description, its \
           `no-std::no-alloc` category and the default row of the README's feature table \
           all promise that the default tier allocates nothing",
          path.display()
        );
      }
      if !line.trim().is_empty() {
        previous = line;
      }
    }
  }
}

/// The names in the manifest's `[features]` table.
///
/// A feature is a line with an `=` before the next table header; continuation lines of a
/// multi-line array have none, and a comment is discarded by what its name would start
/// with rather than by hoping no comment ever contains an `=`.
fn declared_features(manifest: &str) -> Vec<&str> {
  manifest
    .lines()
    .skip_while(|line| line.trim() != "[features]")
    .skip(1)
    .take_while(|line| !line.trim_start().starts_with('['))
    .filter_map(|line| line.split_once('='))
    .map(|(name, _)| name.trim())
    .filter(|name| !name.is_empty() && !name.starts_with('#'))
    .collect()
}

/// The `no-std` job, sliced out of the workflow so that a `--features` written anywhere
/// else in the file cannot answer for a tier this job never builds.
fn no_std_job(workflow: &str) -> String {
  let at = workflow
    .find("\n  no-std:\n")
    .expect("ci.yml declares a `no-std` job");

  workflow[at + 1..]
    .lines()
    .skip(1)
    .take_while(|line| !is_job_header(line))
    .collect::<Vec<_>>()
    .join("\n")
}

/// Whether a line opens a job: a key at the two-space indent every job in this workflow
/// sits at.
fn is_job_header(line: &str) -> bool {
  line.starts_with("  ")
    && !line.starts_with("   ")
    && !line.trim_start().starts_with('#')
    && line.trim_end().ends_with(':')
}

/// Every feature named by a `--features` in the given text, one per name rather than one
/// per flag, so that a cell passing `alloc,jiff` answers for both.
fn features_built_in(job: &str) -> Vec<&str> {
  const FLAG: &str = "--features ";

  job
    .match_indices(FLAG)
    .filter_map(|(at, _)| job.get(at + FLAG.len()..))
    .flat_map(|rest| {
      rest
        .split(|c: char| c.is_whitespace() || c == '\'' || c == '"')
        .next()
        .unwrap_or_default()
        .split(',')
    })
    .filter(|name| !name.is_empty())
    .collect()
}

/// Every `.rs` file under `dir`, recursively.
fn rust_sources(dir: &Path, into: &mut Vec<PathBuf>) {
  for entry in std::fs::read_dir(dir).expect("a readable directory") {
    let path = entry.expect("a readable directory entry").path();
    if path.is_dir() {
      rust_sources(&path, into);
    } else if path.extension().is_some_and(|extension| extension == "rs") {
      into.push(path);
    }
  }
}
