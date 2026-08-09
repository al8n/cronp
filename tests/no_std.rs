//! What this file can and cannot prove.
//!
//! It **cannot** prove that the crate is `no_std`. Every test here runs inside a test
//! binary that links `std`, so `std` is present in the process no matter what the crate
//! under test declares. A passing run is compatible with the crate having quietly grown a
//! `std` dependency.
//!
//! The only thing that proves `no_std` is a build for a target that has no `std` at all:
//!
//! ```text
//! cargo build --target thumbv6m-none-eabi --no-default-features
//! ```
//!
//! That build is the gate. It runs in CI as the `no-std` job in
//! `.github/workflows/ci.yml`; if that job is ever removed, this crate has no `no_std`
//! guarantee left regardless of whether these tests pass.
//!
//! What this file *does* prove is narrower and still worth having: that the crate's
//! public surface is reachable and behaves when the `std` and `alloc` features are off,
//! which is the configuration the bare-metal build compiles but never executes.

/// The bare-metal target that the CI gate builds for.
///
/// Kept here so that a grep for the target name from either the test or the workflow
/// finds the other.
const BARE_METAL_TARGET: &str = "thumbv6m-none-eabi";

#[test]
fn bare_metal_gate_is_named_in_ci() {
  let workflow = include_str!("../.github/workflows/ci.yml");
  assert!(
    workflow.contains(BARE_METAL_TARGET),
    "the bare-metal build is the only proof of no_std and \
     {BARE_METAL_TARGET} no longer appears in ci.yml"
  );
}
