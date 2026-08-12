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
//!
//! # Three instruments, and the one way all three have gone wrong
//!
//! There are three questions here, each answered by an instrument:
//!
//! 1. **which files the library compiles**, so the alloc census reads all of them;
//! 2. **whether an item's `#[cfg]` implies the `alloc` feature**, which is the no-alloc
//!    promise for the half no target build can reach;
//! 3. **whether a matrix cell can turn CI red**, which is what makes a cell a gate.
//!
//! Each was first written against a *proxy* for its question, and each proxy admitted an
//! answer the real question does not. A directory walk stood in for the module graph, so
//! `#[path = "../outside.rs"]` compiled a file the census never opened. Independent cfg
//! atoms stood in for Cargo's feature closure, so `#[cfg(feature = "std")]` was rejected
//! although no build can enable `std` without `alloc`. Substring presence stood in for the
//! command's failure semantics, so `cargo check … || true` satisfied every requirement and
//! could never go red.
//!
//! Two rounds of this were fixed one door at a time — `include!` beside `#[path]`,
//! `continue-on-error` beside `|| true` — and a reviewer found the next door each time.
//! So each instrument below states, next to itself, **the finite list of constructs its
//! own cause implies**, says of each whether it is decided or refused, and has a fixture
//! per member. The list is the thing to check against the cause; a door missing from it is
//! a defect in the list, not a surprise.
//!
//! # Not built under Miri, and the reason is not the subprocess
//!
//! This file names `cronp` nowhere. It parses the crate's *source text* with `syn` and its
//! workflow with `yaml_rust2`, and asks `cargo metadata` for the feature table — so under Miri
//! the interpreter would be running syn, yaml_rust2 and serde, and **zero lines of the crate
//! under test**. Miri exists to find undefined behaviour in code it interprets; here there is
//! none of ours to interpret, so a green cell would assert nothing and a red one would be about
//! somebody else's crate.
//!
//! The visible symptom was narrower than that: `package` shells out to `cargo metadata`, which
//! Miri refuses with `can't call foreign function posix_spawnattr_init`. Gating on the spawn
//! would have been treating the symptom — 18 of these 32 tests reach it today and the other 14
//! would be one refactor away from joining them.
//!
//! Every non-Miri leg still builds and runs the whole file, which is where these assertions were
//! ever meaningful.

#![cfg(not(miri))]

use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  process::Command,
};

use syn::{
  Attribute, Meta, Token,
  parse::{Parse, ParseStream},
  punctuated::Punctuated,
  visit::Visit,
};
use yaml_rust2::{Yaml, YamlLoader};

/// The workflow every check here reads.
const WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");

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
  for target in BARE_METAL_TARGETS {
    assert!(
      WORKFLOW.contains(target),
      "a bare-metal build is the only proof of no_std and {target} no longer appears in \
       ci.yml"
    );
  }
}

#[test]
fn every_declared_feature_says_whether_it_needs_a_host() {
  let package = package();
  let mut declared: Vec<&str> = package.features.keys().map(String::as_str).collect();
  declared.sort_unstable();
  let mut classified: Vec<&str> = FEATURES.iter().map(|(name, _)| *name).collect();
  classified.sort_unstable();

  assert_eq!(
    declared, classified,
    "the features cargo reports for this package and the ones classified in this file have \
     diverged. A new feature is a new tier claim: say here whether it needs a host, and if \
     it does not, give it a cell in the `no-std` job of ci.yml"
  );
}

#[test]
fn every_hostless_feature_has_a_bare_metal_cell() {
  if let Err(complaint) = every_hostless_tier_has_a_cell(WORKFLOW) {
    panic!("{complaint}");
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
  let package = package();
  let census = census_of(&package.root, &read_from_disk, &package.world())
    .unwrap_or_else(|complaint| panic!("{complaint}"));

  assert!(
    census.gaps.is_empty(),
    "the library reaches `alloc` without the `alloc` feature: {}. The crate's description, \
     its `no-std::no-alloc` category and the default row of the README's feature table all \
     promise that the default tier allocates nothing",
    census.gaps.join("; ")
  );

  // The resolver replaced a directory walk, and a resolver that stops early shrinks the
  // census silently — the same false pass in a new place. The walk is kept as the check on
  // the resolver: every file under src/ has to be one the module graph reached.
  let mut walked = Vec::new();
  rust_sources(
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
    &mut walked,
  );
  assert!(
    walked.len() > 1,
    "the walk of src/ found {} files, so it is reading the wrong directory and cannot \
     check the resolver",
    walked.len()
  );
  for path in walked {
    assert!(
      census.files.contains(&path),
      "{} is compiled into no module the resolver reached. Either it is dead source, or \
       the module graph resolution stopped early and the census is reading less than the \
       crate compiles",
      path.display()
    );
  }
}

// ---------------------------------------------------------------------------------------
// What cargo says about this package.
// ---------------------------------------------------------------------------------------

/// The two facts about this package that only cargo can settle, taken from cargo.
///
/// Both were previously read out of `Cargo.toml` by hand — the feature names by a line
/// scan, the crate root by assuming `src/lib.rs`. Neither is a manifest-shaped question:
/// the feature *closure* is cargo's semantics, and the crate root is a `[lib] path` away
/// from the default. `cargo metadata --no-deps` is the authority for both and needs no
/// network, no lockfile and no dependency resolution.
struct Package {
  /// The library target's crate root, where the module graph starts.
  root: PathBuf,
  /// The feature table exactly as declared, including entries like `dep:jiff` and
  /// `jiff?/alloc` that name a dependency rather than a feature of this crate.
  features: BTreeMap<String, Vec<String>>,
}

fn package() -> Package {
  let output = Command::new(env!("CARGO"))
    .args(["metadata", "--no-deps", "--format-version", "1"])
    .arg("--manifest-path")
    .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
    .output()
    .expect("`cargo metadata` runs; it is the authority for this package's features");
  assert!(
    output.status.success(),
    "`cargo metadata` failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );

  let metadata: serde_json::Value =
    serde_json::from_slice(&output.stdout).expect("`cargo metadata` emits JSON");
  let package = metadata["packages"]
    .as_array()
    .expect("`cargo metadata` reports packages")
    .iter()
    .find(|package| package["name"] == env!("CARGO_PKG_NAME"))
    .expect("`cargo metadata --no-deps` reports this package");

  let features = package["features"]
    .as_object()
    .expect("a package has a feature table")
    .iter()
    .map(|(name, enables)| {
      let enables = enables
        .as_array()
        .expect("a feature enables a list")
        .iter()
        .map(|entry| {
          entry
            .as_str()
            .expect("a feature entry is a string")
            .to_owned()
        })
        .collect();
      (name.clone(), enables)
    })
    .collect();

  let root = package["targets"]
    .as_array()
    .expect("a package has targets")
    .iter()
    .find(|target| {
      target["kind"]
        .as_array()
        .is_some_and(|kinds| kinds.iter().any(|kind| kind == "lib"))
    })
    .map(|target| PathBuf::from(target["src_path"].as_str().expect("a target has a source")))
    .expect("this package has a library target");

  Package { root, features }
}

impl Package {
  fn world(&self) -> World {
    World::of(&self.features)
  }
}

fn read_from_disk(path: &Path) -> Option<String> {
  std::fs::read_to_string(path).ok()
}

// ---------------------------------------------------------------------------------------
// Instrument 2: does a gate imply the `alloc` feature?
// ---------------------------------------------------------------------------------------

/// A `#[cfg]` predicate, in the shape `cfg` itself defines it.
///
/// Held as a tree rather than as text because the question asked of it —
/// [`World::implies_the_alloc_feature`] — is about what the predicate admits, and `all`,
/// `any` and `not` differ in exactly that while differing in text by three letters.
#[derive(Clone)]
enum Cfg {
  /// `cfg(true)` and `cfg(false)`, which say nothing about a feature and everything about
  /// whether the item exists.
  Const(bool),
  /// A leaf: `unix`, `feature = "alloc"`, `target_os = "none"`.
  Atom(String),
  /// A conjunction. Also how an item's several `#[cfg]` attributes combine, how the
  /// `#[cfg]`s of the modules above it combine with its own, and how an item with none is
  /// represented — an empty `all()` is true, which is the whole of why an ungated item
  /// fails this census.
  All(Vec<Cfg>),
  Any(Vec<Cfg>),
  Not(Box<Cfg>),
}

impl Parse for Cfg {
  fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
    if input.peek(syn::LitBool) {
      let literal: syn::LitBool = input.parse()?;
      return Ok(Cfg::Const(literal.value()));
    }

    let path: syn::Path = input.parse()?;
    let name = path
      .get_ident()
      .ok_or_else(|| input.error("a cfg option is a single identifier"))?
      .to_string();

    if input.peek(syn::token::Paren) {
      let list;
      syn::parenthesized!(list in input);
      let operands: Punctuated<Cfg, Token![,]> = list.parse_terminated(Cfg::parse, Token![,])?;
      let operands: Vec<Cfg> = operands.into_iter().collect();
      return match name.as_str() {
        "all" => Ok(Cfg::All(operands)),
        "any" => Ok(Cfg::Any(operands)),
        "not" => match <[Cfg; 1]>::try_from(operands) {
          Ok([only]) => Ok(Cfg::Not(Box::new(only))),
          Err(_) => Err(input.error("`not` takes exactly one predicate")),
        },
        _ => Err(input.error("the only cfg operators are `all`, `any` and `not`")),
      };
    }

    if input.peek(Token![=]) {
      let _: Token![=] = input.parse()?;
      let value: syn::LitStr = input.parse()?;
      return Ok(Cfg::Atom(format!("{name} = \"{}\"", value.value())));
    }

    Ok(Cfg::Atom(name))
  }
}

impl std::fmt::Display for Cfg {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Cfg::Const(value) => write!(f, "{value}"),
      Cfg::Atom(text) => f.write_str(text),
      Cfg::Not(inner) => write!(f, "not({inner})"),
      Cfg::All(operands) | Cfg::Any(operands) => {
        f.write_str(if matches!(self, Cfg::All(_)) {
          "all("
        } else {
          "any("
        })?;
        for (position, operand) in operands.iter().enumerate() {
          if position > 0 {
            f.write_str(", ")?;
          }
          write!(f, "{operand}")?;
        }
        f.write_str(")")
      }
    }
  }
}

impl Cfg {
  /// Whether the predicate holds when `on` says which of its atoms are set.
  fn holds(&self, on: &dyn Fn(&str) -> bool) -> bool {
    match self {
      Cfg::Const(value) => *value,
      Cfg::Atom(text) => on(text),
      Cfg::All(operands) => operands.iter().all(|operand| operand.holds(on)),
      Cfg::Any(operands) => operands.iter().any(|operand| operand.holds(on)),
      Cfg::Not(inner) => !inner.holds(on),
    }
  }

  fn atoms(&self, into: &mut Vec<String>) {
    match self {
      Cfg::Const(_) => {}
      Cfg::Atom(text) => into.push(text.clone()),
      Cfg::All(operands) | Cfg::Any(operands) => {
        for operand in operands {
          operand.atoms(into);
        }
      }
      Cfg::Not(inner) => inner.atoms(into),
    }
  }
}

/// The atom the whole census is about.
const ALLOC: &str = "feature = \"alloc\"";

/// The cfg keys that have exactly one value in any one compilation.
///
/// Derived from what the key *is*, not from what this crate happens to write. A target has
/// one architecture, one operating system, one pointer width, one endianness, one
/// environment, one vendor and one ABI, so two atoms on the same key can never both hold
/// and a predicate that needs them to is satisfied by nothing.
///
/// `target_family`, `target_feature` and `target_has_atomic` are deliberately absent: each
/// can hold several values at once. Leaving them out means their atoms are enumerated as
/// independent, which considers *more* configurations than exist — and a counterexample
/// found in a configuration that cannot exist is a false alarm, never a false pass. That
/// direction is the one this list may err in.
const SINGLE_VALUED_CFG_KEYS: &[&str] = &[
  "target_arch",
  "target_os",
  "target_pointer_width",
  "target_endian",
  "target_env",
  "target_vendor",
  "target_abi",
];

/// Everything that decides which assignments of cfg atoms a real build can produce.
///
/// # The list this instrument's cause implies
///
/// The question is "can cargo and rustc between them produce a build where this item
/// exists and the `alloc` feature is off". An enumeration over independent atoms answers a
/// different, larger question, and every difference between the two is a fact that ties
/// atoms together. There are exactly four kinds of such fact, and one construct that
/// rewrites the gate before any of them apply:
///
/// 1. **Cargo feature implications.** `std = ["alloc", …]` means no build has `std`
///    without `alloc`. *Decided*: the table is taken from `cargo metadata` and closed
///    transitively.
/// 2. **Feature entries that are not features of this crate.** `dep:jiff`, `jiff/static`
///    and `jiff?/alloc` name a dependency; they tie none of this crate's atoms together.
///    *Decided*: excluded, and an entry that is neither is refused rather than guessed at.
/// 3. **Single-valued cfg keys.** Two `target_os` atoms cannot both hold. *Decided*, over
///    [`SINGLE_VALUED_CFG_KEYS`], whose own boundary is documented there.
/// 4. **`unix` and `windows`.** Never both, though a bare-metal target is neither.
///    *Decided*.
/// 5. **`cfg_attr`.** Not a fact about atoms but a rewrite of the gate:
///    `#[cfg_attr(P, cfg(Q))]` gates the item on `¬P ∨ Q`, because when `P` fails the
///    attribute is never applied and the item is unconditional. *Decided*, recursively —
///    and a `cfg_attr` that can produce a `#[path]` is refused by instrument 1, since that
///    makes the *file set* configuration-dependent rather than the gate.
///
/// Every other cfg option is a free atom, which is the right answer for all of them: a
/// `--cfg` from `build.rs` (this crate emits `tarpaulin`), `docsrs`, and `test` are each
/// independently settable. `test` is worth naming because being strict about it is a
/// choice rather than an oversight: `#[cfg(test)] extern crate alloc;` would be reported,
/// though a test build is not the artifact the no-alloc promise is about. Reporting it
/// costs a false alarm and a deliberate edit here; not reporting it would need this file
/// to decide which configurations the promise covers, which is a larger claim than it
/// makes.
struct World {
  /// `a` implies `b`, transitively closed, in atom form.
  implications: Vec<(String, String)>,
}

impl World {
  /// The world Cargo's feature table describes.
  fn of(features: &BTreeMap<String, Vec<String>>) -> World {
    let mut edges: Vec<(String, String)> = Vec::new();
    for (feature, enables) in features {
      for entry in enables {
        // `dep:x` activates a dependency; `x/y` and `x?/y` enable a feature of one. None
        // of the three is a feature of this crate, so none constrains an atom here.
        if entry.starts_with("dep:") || entry.contains('/') {
          continue;
        }
        assert!(
          features.contains_key(entry),
          "the `{feature}` feature enables `{entry}`, which is neither a feature of this \
           crate nor a dependency entry. This test would have to guess what it ties to"
        );
        edges.push((feature.clone(), entry.clone()));
      }
    }

    // Transitive closure, so an implication may be read off one pair rather than searched
    // for through a chain.
    loop {
      let mut grown = Vec::new();
      for (from, through) in &edges {
        for (start, to) in &edges {
          if start == through && !edges.iter().any(|(a, b)| a == from && b == to) && from != to {
            grown.push((from.clone(), to.clone()));
          }
        }
      }
      if grown.is_empty() {
        break;
      }
      edges.extend(grown);
    }

    World {
      implications: edges
        .into_iter()
        .map(|(from, to)| (feature_atom(&from), feature_atom(&to)))
        .collect(),
    }
  }

  /// Whether every configuration this world admits in which `gate` holds is one with the
  /// `alloc` feature on.
  ///
  /// Decided rather than recognised. The atoms are enumerated over every truth assignment,
  /// the assignments this world forbids are dropped, and the implication is the absence of
  /// a counterexample — an assignment where the item exists and the feature is off. That is
  /// what makes the answer indifferent to how the predicate is written: `any(feature =
  /// "alloc", feature = "tz-static")` has a counterexample and `not(feature = "alloc")` is
  /// a counterexample everywhere it holds, while both contain the literal a text scan looks
  /// for. A predicate no configuration satisfies — `all(unix, not(unix))`, `false` —
  /// implies this vacuously, and correctly: the item it gates is compiled nowhere.
  fn implies_the_alloc_feature(&self, gate: &Cfg) -> Result<bool, String> {
    let mut atoms = vec![ALLOC.to_owned()];
    gate.atoms(&mut atoms);
    atoms.sort_unstable();
    atoms.dedup();
    if atoms.len() > 16 {
      return Err(format!(
        "`{gate}` names {} distinct cfg options, more than this decision procedure \
         enumerates. Either the gate wants splitting or this bound wants raising",
        atoms.len()
      ));
    }

    let at = |wanted: &str| atoms.iter().position(|atom| atom == wanted);
    let alloc_at = at(ALLOC).ok_or("the alloc atom is seeded above and cannot be missing")?;

    // The pairs and groups that make an assignment impossible, resolved to bit positions
    // once rather than per assignment.
    let implications: Vec<(usize, usize)> = self
      .implications
      .iter()
      .filter_map(|(from, to)| Some((at(from)?, at(to)?)))
      .collect();
    let mut exclusive: Vec<Vec<usize>> = Vec::new();
    for key in SINGLE_VALUED_CFG_KEYS {
      let group: Vec<usize> = atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| atom.starts_with(&format!("{key} = ")))
        .map(|(position, _)| position)
        .collect();
      if group.len() > 1 {
        exclusive.push(group);
      }
    }
    if let (Some(unix), Some(windows)) = (at("unix"), at("windows")) {
      exclusive.push(vec![unix, windows]);
    }

    for assignment in 0u32..(1u32 << atoms.len()) {
      let set = |position: usize| assignment & (1 << position) != 0;
      if implications
        .iter()
        .any(|(from, to)| set(*from) && !set(*to))
      {
        continue;
      }
      if exclusive
        .iter()
        .any(|group| group.iter().filter(|position| set(**position)).count() > 1)
      {
        continue;
      }
      let holds = |atom: &str| at(atom).is_some_and(set);
      if gate.holds(&holds) && !set(alloc_at) {
        return Ok(false);
      }
    }
    Ok(true)
  }
}

fn feature_atom(feature: &str) -> String {
  format!("feature = \"{feature}\"")
}

// ---------------------------------------------------------------------------------------
// Instrument 1: which files does the library compile?
// ---------------------------------------------------------------------------------------

/// The macros this crate invokes, and the reason a list is needed at all.
///
/// A macro's expansion is not in the source, so nothing here can see what it produces —
/// including an `extern crate alloc`. Every invocation is therefore a hazard, and the
/// exemptions have to be named. These are the ones this crate invokes: all of `core`'s or
/// `std`'s except `tz::get!`, which is jiff's, and every one of them expands to an
/// expression rather than to an item. A macro this list does not name is refused, which is
/// the deliberate edit that makes someone say why the new one is inert.
const INERT_MACROS: &[&str] = &[
  "assert",
  "assert_eq",
  "assert_ne",
  "debug_assert",
  "format",
  "format_args",
  "include_str",
  "matches",
  "panic",
  "vec",
  "write",
  "tz::get",
];

/// What the alloc census found.
#[derive(Debug)]
struct CensusResult {
  /// Every file the module graph reached, so that a resolver that stopped early can be
  /// caught by comparing this against a walk of the directory.
  files: Vec<PathBuf>,
  /// Every `extern crate alloc` whose gate admits a build with the feature off.
  gaps: Vec<String>,
  /// Which of [`INERT_MACROS`] the library actually invokes, so that an exemption whose
  /// reason has expired is removed rather than left standing.
  inert_invoked: Vec<String>,
}

/// Resolve the library's file set the way rustc does, and census every file in it.
///
/// # The list this instrument's cause implies
///
/// The question is "which files does rustc compile into this library", and a walk of a
/// directory answers a different one. rustc starts at the crate root and follows the
/// module graph, so the complete list of ways a file enters the crate is the list of ways
/// the graph can name one:
///
/// 1. **The crate root itself**, which `[lib] path` can move out of `src/`. *Decided*:
///    taken from `cargo metadata`, not assumed.
/// 2. **`mod name;`**, resolved against the module's own directory as `name.rs` or
///    `name/mod.rs`. *Decided*: resolved, and exactly one of the two must exist.
/// 3. **`#[path = "…"] mod name;`**, which names any file anywhere. *Decided*: followed,
///    and the file is censused like any other.
/// 4. **`#[path = "…"] mod name { … }`**, where the path redirects the directory the
///    module's *children* resolve against, moving a whole subtree. *Decided*: followed.
/// 5. **`#[cfg_attr(P, path = "…")]`**, which makes the file set depend on the
///    configuration. *Refused*: this census answers one question about one file set, and
///    there would be two.
/// 6. **`include!("…")`**, which splices items from a file the graph never names — the
///    build-script-plus-`OUT_DIR` case included. *Refused*.
/// 7. **A macro invocation**, whose expansion can contain a `mod`, an `include!` or an
///    `extern crate` and is not in the source. *Refused* unless the macro is named in
///    [`INERT_MACROS`].
///
/// `include_str!` and `include_bytes!` are on none of these lists on purpose: each expands
/// to a literal in expression position, so it reads a file without that file becoming
/// source. `#![doc = include_str!("../README.md")]` is the crate's own use of that, and it
/// introduces no item.
///
/// The remaining gap is stated rather than closed: an `extern crate alloc` that a macro
/// listed in [`INERT_MACROS`] expands to would not be seen. Each of those is `core`'s,
/// `std`'s or jiff's, and expands to an expression.
fn census_of(
  root: &Path,
  open: &dyn Fn(&Path) -> Option<String>,
  world: &World,
) -> Result<CensusResult, String> {
  let root = normalised(root);
  let root_directory = root
    .parent()
    .ok_or("the crate root has no directory to resolve modules against")?
    .to_path_buf();

  let mut queue = vec![(root, root_directory, Vec::new())];
  let mut files: Vec<PathBuf> = Vec::new();
  let mut gaps = Vec::new();
  let mut inert_invoked: Vec<String> = Vec::new();

  while let Some((file, directory, gate)) = queue.pop() {
    if files.contains(&file) {
      continue;
    }
    files.push(file.clone());

    let text = open(&file).ok_or_else(|| {
      format!(
        "{} is part of the module graph and could not be read",
        file.display()
      )
    })?;
    let parsed = syn::parse_file(&text)
      .map_err(|error| format!("{} does not parse as Rust: {error}", file.display()))?;

    let mut walk = Walk {
      open,
      directory,
      gate,
      queue: Vec::new(),
      found: Vec::new(),
      refusals: Vec::new(),
      inert_invoked: Vec::new(),
    };
    walk.visit_file(&parsed);
    if let Some(refusal) = walk.refusals.first() {
      return Err(format!("{}: {refusal}", file.display()));
    }

    for (named, gates) in walk.found {
      let ungated = gates.is_empty();
      let gate = conjunction(gates);
      if !world.implies_the_alloc_feature(&gate)? {
        let why = if ungated {
          "carries no `#[cfg]` at all".to_owned()
        } else {
          format!("is gated by `{gate}`, which admits a build with the `alloc` feature off")
        };
        gaps.push(format!("{}: {named} {why}", file.display()));
      }
    }
    queue.extend(walk.queue);
    inert_invoked.extend(walk.inert_invoked);
  }

  files.sort();
  inert_invoked.sort();
  inert_invoked.dedup();
  Ok(CensusResult {
    files,
    gaps,
    inert_invoked,
  })
}

/// One file's worth of the walk: the `extern crate alloc` items in it, the files its `mod`
/// items name, and the constructs that stop the walk from being able to answer.
struct Walk<'a> {
  open: &'a dyn Fn(&Path) -> Option<String>,
  /// The directory the current module resolves its children against.
  directory: PathBuf,
  /// The `#[cfg]`s of every module above the current one, which gate everything in it.
  gate: Vec<Cfg>,
  queue: Vec<(PathBuf, PathBuf, Vec<Cfg>)>,
  found: Vec<(String, Vec<Cfg>)>,
  refusals: Vec<String>,
  inert_invoked: Vec<String>,
}

impl Walk<'_> {
  fn refuse(&mut self, refusal: String) {
    self.refusals.push(refusal);
  }
}

impl<'ast> Visit<'ast> for Walk<'_> {
  fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
    if item.ident != "alloc" {
      return;
    }
    let named = match &item.rename {
      Some((_, alias)) => format!("`extern crate alloc as {alias}`"),
      None => "`extern crate alloc`".to_owned(),
    };
    match gates_and_path(&item.attrs) {
      Ok((gates, _)) => {
        let mut gate = self.gate.clone();
        gate.extend(gates);
        self.found.push((named, gate));
      }
      Err(refusal) => self.refuse(refusal),
    }
  }

  fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
    let (gates, path) = match gates_and_path(&item.attrs) {
      Ok(both) => both,
      Err(refusal) => return self.refuse(refusal),
    };
    let mut gate = self.gate.clone();
    gate.extend(gates);
    let name = item.ident.to_string();

    let Some((_, items)) = &item.content else {
      // An out-of-line module names a file, and `#[path]` names any file at all.
      let file = match &path {
        Some(path) => normalised(&self.directory.join(path)),
        None => {
          let flat = normalised(&self.directory.join(format!("{name}.rs")));
          let nested = normalised(&self.directory.join(&name).join("mod.rs"));
          match ((self.open)(&flat).is_some(), (self.open)(&nested).is_some()) {
            (true, false) => flat,
            (false, true) => nested,
            (true, true) => {
              return self.refuse(format!(
                "`mod {name};` has both {} and {} to choose from, which rustc rejects and \
                 this resolver will not guess at",
                flat.display(),
                nested.display()
              ));
            }
            (false, false) => {
              return self.refuse(format!(
                "`mod {name};` names neither {} nor {}",
                flat.display(),
                nested.display()
              ));
            }
          }
        }
      };
      // A module whose file is `mod.rs` owns that file's directory; any other file owns a
      // subdirectory named after the module. This is rustc's rule and it is what makes
      // `#[path]` move a whole subtree rather than one file.
      let directory = match file.file_name().and_then(|name| name.to_str()) {
        Some("mod.rs") => file.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
        _ => file.parent().unwrap_or_else(|| Path::new("")).join(&name),
      };
      self.queue.push((file, directory, gate));
      return;
    };

    // An inline module's children resolve against a subdirectory named after it, or
    // against whatever `#[path]` says instead.
    let directory = match &path {
      Some(path) => normalised(&self.directory.join(path)),
      None => normalised(&self.directory.join(&name)),
    };
    let outer_directory = std::mem::replace(&mut self.directory, directory);
    let outer_gate = std::mem::replace(&mut self.gate, gate);
    for item in items {
      self.visit_item(item);
    }
    self.directory = outer_directory;
    self.gate = outer_gate;
  }

  fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
    let path = invocation
      .path
      .segments
      .iter()
      .map(|segment| segment.ident.to_string())
      .collect::<Vec<_>>()
      .join("::");
    let inert = INERT_MACROS.iter().find(|inert| {
      path == **inert
        || ["std", "core", "alloc"]
          .iter()
          .any(|sysroot| path == format!("{sysroot}::{inert}"))
    });
    if let Some(inert) = inert {
      self.inert_invoked.push((*inert).to_owned());
    } else {
      self.refuse(format!(
        "`{path}!` expands to source this census never sees, and it is not one of the \
         macros named as inert. A macro can expand to a `mod`, an `include!` or an \
         `extern crate`, so this one has to be looked at rather than assumed"
      ));
    }
    syn::visit::visit_macro(self, invocation);
  }
}

/// `path` with `.` and `..` folded away.
///
/// The module graph joins names onto directories, so `#[path = "../generated.rs"]` under
/// `src/` produces `src/../generated.rs`. Folding is textual, which is what rustc's own
/// joining is, and it is what makes one file reached two ways one entry rather than two.
fn normalised(path: &Path) -> PathBuf {
  let mut folded = PathBuf::new();
  for part in path {
    match part.to_str() {
      Some("..") => {
        folded.pop();
      }
      Some(".") => {}
      _ => folded.push(part),
    }
  }
  folded
}

/// Several gates on one item are a conjunction; one is itself, and reads back as the
/// source wrote it.
fn conjunction(mut gates: Vec<Cfg>) -> Cfg {
  match gates.len() {
    1 => gates.remove(0),
    _ => Cfg::All(gates),
  }
}

/// The gates an item's attributes put on it, and the `#[path]` they give it.
///
/// One pass for both because the two questions share the constructs that answer them: a
/// `#[cfg_attr]` can produce either, and a `#[cfg_attr]` that can produce a `#[path]` is
/// the one shape neither instrument may decide alone.
fn gates_and_path(attributes: &[Attribute]) -> Result<(Vec<Cfg>, Option<String>), String> {
  let mut gates = Vec::new();
  let mut path = None;
  for attribute in attributes {
    if attribute.path().is_ident("cfg") {
      let Meta::List(list) = &attribute.meta else {
        return Err("a `#[cfg]` that is not a predicate list".to_owned());
      };
      gates.push(
        syn::parse2::<Cfg>(list.tokens.clone())
          .map_err(|error| format!("a `#[cfg]` this census cannot read: {error}"))?,
      );
    } else if attribute.path().is_ident("cfg_attr") {
      let Meta::List(list) = &attribute.meta else {
        return Err("a `#[cfg_attr]` that is not a list".to_owned());
      };
      let conditional: CfgAttr = syn::parse2(list.tokens.clone())
        .map_err(|error| format!("a `#[cfg_attr]` this census cannot read: {error}"))?;
      gates.extend(conditional.gates()?);
    } else if attribute.path().is_ident("path") {
      let Meta::NameValue(named) = &attribute.meta else {
        return Err("a `#[path]` that is not `path = \"…\"`".to_owned());
      };
      let syn::Expr::Lit(literal) = &named.value else {
        return Err("a `#[path]` whose value is not a literal".to_owned());
      };
      let syn::Lit::Str(text) = &literal.lit else {
        return Err("a `#[path]` whose value is not a string".to_owned());
      };
      path = Some(text.value());
    }
  }
  Ok((gates, path))
}

/// `#[cfg_attr(predicate, applied…)]`, held apart so the gate it produces can be derived
/// rather than guessed at.
struct CfgAttr {
  predicate: Cfg,
  applied: Vec<Meta>,
}

impl Parse for CfgAttr {
  fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
    let predicate: Cfg = input.parse()?;
    let _: Token![,] = input.parse()?;
    let applied: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;
    Ok(CfgAttr {
      predicate,
      applied: applied.into_iter().collect(),
    })
  }
}

impl CfgAttr {
  /// The gate this attribute puts on its item: `¬P ∨ Q` for each `cfg(Q)` it applies,
  /// because when `P` does not hold the attribute is not applied and the item is left
  /// unconditional.
  fn gates(&self) -> Result<Vec<Cfg>, String> {
    let unless = Cfg::Not(Box::new(self.predicate.clone()));
    let mut gates = Vec::new();
    for applied in &self.applied {
      if applied.path().is_ident("path") {
        return Err(
          "a `#[cfg_attr(…, path = \"…\")]` makes which file the crate compiles depend on \
           the configuration, and this census answers for one file set"
            .to_owned(),
        );
      }
      if applied.path().is_ident("cfg") {
        let Meta::List(list) = applied else {
          return Err("a `#[cfg_attr]` applying a `cfg` that is not a list".to_owned());
        };
        let inner = syn::parse2::<Cfg>(list.tokens.clone())
          .map_err(|error| format!("a `#[cfg_attr]` this census cannot read: {error}"))?;
        gates.push(Cfg::Any(vec![unless.clone(), inner]));
      } else if applied.path().is_ident("cfg_attr") {
        let Meta::List(list) = applied else {
          return Err("a nested `#[cfg_attr]` that is not a list".to_owned());
        };
        let nested: CfgAttr = syn::parse2(list.tokens.clone())
          .map_err(|error| format!("a nested `#[cfg_attr]` this census cannot read: {error}"))?;
        for gate in nested.gates()? {
          gates.push(Cfg::Any(vec![unless.clone(), gate]));
        }
      }
      // Any other attribute is not a gate, and `#[path]` is the only other one whose
      // meaning this census depends on.
    }
    Ok(gates)
  }
}

// ---------------------------------------------------------------------------------------
// Instrument 3: can the cell turn CI red?
// ---------------------------------------------------------------------------------------

/// One `include` row of the `no-std` job's matrix.
struct Cell {
  target: String,
  /// The `--features` flag exactly as the row writes it. Empty is the cell that passes
  /// none, which is how the `default` tier is built.
  flag: String,
  /// The feature names that flag selects.
  features: Vec<String>,
}

/// The characters that stop a `run:` from being one command whose exit status is the
/// step's.
///
/// GitHub runs a `run:` as a shell script and fails the step on a non-zero exit. Every one
/// of these either adds a second command whose status can replace the first's, or hides
/// one inside a substitution: `||` and `&&` and `;` and a newline sequence commands, `|`
/// makes the pipeline's last exit status the answer, `&` detaches, `(` and `)` and `` ` ``
/// and `$` and quotes open something that is no longer this command, and `\` continues a
/// line. Requiring their absence is what turns "the string contains `cargo check`" into
/// "the step runs `cargo check` and reports what it said".
const SHELL_OPERATORS: &[char] = &[
  '|', '&', ';', '<', '>', '(', ')', '{', '}', '$', '`', '\\', '"', '\'', '\n', '\r',
];

/// The keys under a job that change what a red cargo check means.
///
/// `if` decides whether the step or job runs at all; `continue-on-error` decides whether
/// its failure fails anything; `shell` replaces the interpreter and with it the rule that
/// a non-zero exit fails the step; `working-directory` changes which package is being
/// checked, so the cell would answer for something else.
const KEYS_THAT_UNDO_A_FAILURE: &[&str] =
  &["if", "continue-on-error", "shell", "working-directory"];

/// Whether every tier that claims to need no host is built by a cell that could refuse it.
///
/// # The list this instrument's cause implies
///
/// The question is "would this tier's failure turn CI red", and it has three parts: the
/// cell has to exist, it has to build that tier for a target that has no host, and its
/// failure has to reach the job's conclusion. Text answers none of them.
///
/// **That the cell exists and builds the tier**, which a scan of the job's bytes cannot
/// tell from a comment, from a matrix key no step reads, or from a row aimed at a host
/// target. *Decided*: the rows come from `jobs.no-std.strategy.matrix.include`, every key
/// they carry has to be interpolated somewhere in the job, and the tier has to be in the
/// `features` of a row whose `target` is bare metal. Anything under `matrix` beside
/// `include` is *refused*: an axis multiplies with the include rows, and an `exclude` takes
/// rows away, so either would leave this file reading cells CI does not run.
///
/// **That the command is the cargo check.** Everything between the matrix row and the
/// process's exit status is either a YAML key or a shell operator, so both lists are
/// finite: [`KEYS_THAT_UNDO_A_FAILURE`] anywhere under the job is *refused*, and so is any
/// of [`SHELL_OPERATORS`] in the `run` — in the template *or* in any matrix value the
/// template interpolates, since the command as executed is both.
///
/// **That nothing else swallows the result.** `timeout-minutes` and `strategy.fail-fast`
/// are the two that look like they might and do not: a timeout fails the step, and
/// `fail-fast` cancels *other* cells rather than forgiving this one. *Decided*: allowed,
/// with a fixture each so the decision is exercised rather than asserted.
fn every_hostless_tier_has_a_cell(workflow: &str) -> Result<(), String> {
  let documents =
    YamlLoader::load_from_str(workflow).map_err(|error| format!("ci.yml is not YAML: {error}"))?;
  let document = documents
    .first()
    .ok_or("ci.yml holds no YAML document at all")?;

  let job = &document["jobs"]["no-std"];
  if job.is_badvalue() {
    return Err(
      "ci.yml declares no `no-std` job. That job is the only proof this crate is no_std".to_owned(),
    );
  }

  for key in KEYS_THAT_UNDO_A_FAILURE {
    if names_a_key(job, key) {
      return Err(format!(
        "the `no-std` job carries a `{key}`. A cell that can be skipped, forgiven, run by \
         another interpreter or pointed at another directory builds a tier without gating \
         it — which is the state this whole file exists to make impossible"
      ));
    }
  }

  let cells = cells_of(job)?;
  let read = interpolations_normalised(&scalars_of(job).join("\n"));

  // A matrix key the job never interpolates builds nothing, whatever it says. Derived from
  // the rows rather than listed here, so a key added to a row is covered the day it lands.
  let mut keys: Vec<&String> = cells_keys(job)?;
  keys.sort_unstable();
  keys.dedup();
  for key in keys {
    if !read.contains(&format!("${{{{matrix.{key}}}}}")) {
      return Err(format!(
        "the `no-std` matrix carries a `{key}` no step of the job reads. A value nothing \
         interpolates builds nothing, so it cannot stand in for a tier's cell"
      ));
    }
  }

  // ...and the command has to be the one this file's whole claim rests on.
  let mut commands = job["steps"]
    .as_vec()
    .ok_or("the `no-std` job has no `steps`")?
    .iter()
    .filter_map(|step| step["run"].as_str())
    .filter(|run| run.contains("cargo check"));
  let run = commands
    .next()
    .ok_or("no step of the `no-std` job runs `cargo check`")?;
  if commands.next().is_some() {
    return Err(
      "more than one step of the `no-std` job runs `cargo check`, so which one a cell \
       reaches is no longer something this test can say"
        .to_owned(),
    );
  }
  command_is_the_check(run, &cells)?;

  let bare_metal = |cell: &&Cell| BARE_METAL_TARGETS.contains(&cell.target.as_str());
  for (feature, host) in FEATURES {
    if matches!(host, Host::Required) {
      continue;
    }
    let covered = if *feature == "default" {
      // `default = []`, so this tier's cell is the one that passes no feature at all
      // rather than one with a name to look for.
      cells
        .iter()
        .filter(bare_metal)
        .any(|cell| cell.flag.trim().is_empty())
    } else {
      cells
        .iter()
        .filter(bare_metal)
        .any(|cell| cell.features.iter().any(|name| name == feature))
    };
    if !covered {
      return Err(format!(
        "`{feature}` claims to need no host and no cell of the `no-std` job builds it for \
         a bare-metal target. The tier's own tests cannot stand in: they run on a host, in \
         a graph that supplies the std and alloc the tier disclaims"
      ));
    }
  }
  Ok(())
}

/// Whether the step's `run` is the cargo check itself rather than a script containing one.
fn command_is_the_check(run: &str, cells: &[Cell]) -> Result<(), String> {
  let template = without_interpolations(run);
  if let Some(operator) = template.chars().find(|c| SHELL_OPERATORS.contains(c)) {
    return Err(format!(
      "the `no-std` job's cargo command contains `{operator}`, so the step's exit status is \
       no longer the cargo check's: `{run}`"
    ));
  }
  if !template.trim().starts_with("cargo check") {
    return Err(format!(
      "the `no-std` job's cargo check is not the whole of what the step runs: `{run}`"
    ));
  }
  for required in [
    "--lib",
    "--no-default-features",
    "${{matrix.features}}",
    "--target ${{matrix.target}}",
  ] {
    if !interpolations_normalised(run).contains(required) {
      return Err(format!(
        "the `no-std` job's cargo command does not pass `{required}`, so its cells no \
         longer say what this test reads them as saying: `{run}`"
      ));
    }
  }

  // The command as executed is the template plus the values spliced into it, so a value
  // carrying an operator is the same defect one indirection along.
  for key in interpolated_keys(run) {
    for cell in cells {
      let value = match key.as_str() {
        "features" => &cell.flag,
        "target" => &cell.target,
        _ => continue,
      };
      if let Some(operator) = value.chars().find(|c| SHELL_OPERATORS.contains(c)) {
        return Err(format!(
          "a cell interpolates `{value}` into the cargo command, and `{operator}` in it \
           means the step's exit status is no longer the check's"
        ));
      }
    }
  }
  Ok(())
}

/// The `include` rows of the `no-std` job's matrix.
fn cells_of(job: &Yaml) -> Result<Vec<Cell>, String> {
  let matrix = &job["strategy"]["matrix"];
  let keys = matrix
    .as_hash()
    .ok_or("the `no-std` job has no `strategy.matrix`")?;
  for key in keys.keys() {
    if key.as_str() != Some("include") {
      return Err(format!(
        "the `no-std` matrix has a `{key:?}` axis beside `include`. Axes multiply with the \
         include rows and this test does not model that, so it would be reading cells that \
         are not the ones CI runs"
      ));
    }
  }

  matrix["include"]
    .as_vec()
    .ok_or("the `no-std` matrix has no `include` rows")?
    .iter()
    .map(|row| {
      let string = |key: &str| {
        row[key]
          .as_str()
          .ok_or_else(|| format!("an `include` row of the `no-std` job has no `{key}`"))
      };
      // Nothing below reads `tier` — it names the cell in the job's title. Requiring it
      // anyway is what keeps a half-written row from being read as a whole one.
      string("tier")?;
      let flag = string("features")?.to_owned();
      Ok(Cell {
        target: string("target")?.to_owned(),
        features: features_of(&flag)?,
        flag,
      })
    })
    .collect()
}

/// Every key the `include` rows use, which is every name a step could interpolate.
fn cells_keys(job: &Yaml) -> Result<Vec<&String>, String> {
  let mut keys = Vec::new();
  for row in job["strategy"]["matrix"]["include"]
    .as_vec()
    .ok_or("the `no-std` matrix has no `include` rows")?
  {
    for key in row
      .as_hash()
      .ok_or("an `include` row of the `no-std` job is not a mapping")?
      .keys()
    {
      match key {
        Yaml::String(name) => keys.push(name),
        other => return Err(format!("an `include` row has a non-string key {other:?}")),
      }
    }
  }
  Ok(keys)
}

/// The features a cell's `--features` flag names.
///
/// Strict about the flag's shape rather than hunting names inside it: a cell is one short
/// hand-written string, so a form this does not understand is a deliberate edit, and it
/// should come through here rather than be read as building something it does not.
fn features_of(flag: &str) -> Result<Vec<String>, String> {
  let mut names = Vec::new();
  let mut words = flag.split_whitespace();
  while let Some(word) = words.next() {
    let list = match word.strip_prefix("--features=") {
      Some(list) => list,
      None if word == "--features" => words
        .next()
        .ok_or_else(|| format!("a cell's `{flag}` ends in a `--features` with no list"))?,
      None => {
        return Err(format!(
          "a cell passes `{word}`, which this test does not model. Its cells are read as \
           `--features <list>` and nothing else"
        ));
      }
    };
    names.extend(
      list
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_owned),
    );
  }
  Ok(names)
}

/// Whether `name` is a mapping key anywhere under `node`.
fn names_a_key(node: &Yaml, name: &str) -> bool {
  match node {
    Yaml::Array(items) => items.iter().any(|item| names_a_key(item, name)),
    Yaml::Hash(entries) => entries
      .iter()
      .any(|(key, value)| key.as_str() == Some(name) || names_a_key(value, name)),
    _ => false,
  }
}

/// Every scalar *value* under `node`.
///
/// Values only, because an interpolation is something a step is given and never something
/// a key is called. This is the job's text with its comments dropped, which is the whole
/// difference between what CI reads and what a scan of the file sees.
fn scalars_of(node: &Yaml) -> Vec<String> {
  match node {
    Yaml::String(text) => vec![text.clone()],
    Yaml::Array(items) => items.iter().flat_map(scalars_of).collect(),
    Yaml::Hash(entries) => entries
      .iter()
      .flat_map(|(_, value)| scalars_of(value))
      .collect(),
    _ => Vec::new(),
  }
}

/// `text` with the spaces inside every `${{ … }}` removed, so that the workflow may write
/// the interpolation either way and this file need not know which.
fn interpolations_normalised(text: &str) -> String {
  rewrite_interpolations(text, |inside| {
    let mut out = String::from("${{");
    out.extend(inside.chars().filter(|c| !c.is_whitespace()));
    out.push_str("}}");
    out
  })
}

/// `text` with every `${{ … }}` replaced by a space, which is the command as the shell
/// would see it if every interpolation were an ordinary word.
fn without_interpolations(text: &str) -> String {
  rewrite_interpolations(text, |_| " ".to_owned())
}

/// The `matrix.<key>` names a template interpolates.
fn interpolated_keys(text: &str) -> Vec<String> {
  let mut keys = Vec::new();
  rewrite_interpolations(text, |inside| {
    let inside: String = inside.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(key) = inside.strip_prefix("matrix.") {
      keys.push(key.to_owned());
    }
    String::new()
  });
  keys
}

fn rewrite_interpolations(text: &str, mut rewrite: impl FnMut(&str) -> String) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find("${{") {
    out.push_str(&rest[..open]);
    let after = &rest[open + 3..];
    match after.find("}}") {
      Some(close) => {
        out.push_str(&rewrite(&after[..close]));
        rest = &after[close + 2..];
      }
      None => {
        out.push_str(&rest[open..]);
        return out;
      }
    }
  }
  out.push_str(rest);
  out
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

// ---------------------------------------------------------------------------------------
// Fixtures: one per member of each of the three lists above.
// ---------------------------------------------------------------------------------------

/// A crate that exists only as text, so that a fixture can name a file outside `src/`
/// without one being there.
fn crate_of(files: &[(&str, &str)]) -> impl Fn(&Path) -> Option<String> {
  let files: BTreeMap<PathBuf, String> = files
    .iter()
    .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
    .collect();
  move |path: &Path| files.get(path).cloned()
}

/// The world the fixtures ask their questions in: this crate's own feature table, so that
/// `std` implies `alloc` here exactly as it does in a build.
fn fixture_world() -> World {
  World::of(&package().features)
}

fn census(files: &[(&str, &str)]) -> Result<CensusResult, String> {
  census_of(Path::new("src/lib.rs"), &crate_of(files), &fixture_world())
}

// --- instrument 1, member 1: the crate root can be moved by `[lib] path` -----------------

#[test]
fn the_crate_root_is_the_one_cargo_reports() {
  let package = package();
  assert!(
    package.root.ends_with("src/lib.rs"),
    "cargo reports {} as the library's crate root, and this file assumed src/lib.rs",
    package.root.display()
  );
  // The census starts wherever cargo says, so a `[lib] path` move is followed rather than
  // missed. Proven by starting it somewhere else and watching it read that file instead.
  let elsewhere = census_of(
    Path::new("other/root.rs"),
    &crate_of(&[("other/root.rs", "extern crate alloc;\n")]),
    &fixture_world(),
  )
  .expect("a crate root outside src/ is still a crate root");
  assert_eq!(elsewhere.files, vec![PathBuf::from("other/root.rs")]);
  assert_eq!(elsewhere.gaps.len(), 1);
}

// --- instrument 1, member 2: `mod name;` resolves to name.rs or name/mod.rs --------------

#[test]
fn a_conventional_module_is_resolved_both_ways_round() {
  for (path, text) in [
    ("src/flat.rs", "extern crate alloc;\n"),
    ("src/flat/mod.rs", "extern crate alloc;\n"),
  ] {
    let census = census(&[("src/lib.rs", "mod flat;\n"), (path, text)])
      .expect("a module resolved the way rustc resolves it");
    assert_eq!(census.files.len(), 2, "{path} was not reached");
    assert_eq!(census.gaps.len(), 1, "{path} was reached but not censused");
  }
}

#[test]
fn a_module_with_neither_file_or_both_is_refused() {
  let missing = census(&[("src/lib.rs", "mod gone;\n")]).expect_err("a module with no file");
  assert!(missing.contains("names neither"), "{missing}");

  let both = census(&[
    ("src/lib.rs", "mod twice;\n"),
    ("src/twice.rs", ""),
    ("src/twice/mod.rs", ""),
  ])
  .expect_err("a module rustc itself would reject");
  assert!(both.contains("both"), "{both}");
}

// --- instrument 1, member 3: `#[path]` on an out-of-line module --------------------------

/// The finding that reopened this file: a file compiled by rustc, outside `src/`, that a
/// directory walk never opens, holding an ungated `extern crate alloc;`.
#[test]
fn a_path_attribute_pointing_outside_src_is_followed_and_censused() {
  let census = census(&[
    (
      "src/lib.rs",
      "#[path = \"../generated.rs\"]\nmod generated;\n",
    ),
    ("generated.rs", "extern crate alloc;\n"),
  ])
  .expect("a `#[path]` module is a module");
  assert!(
    census.files.contains(&PathBuf::from("generated.rs")),
    "the file rustc compiles was not read: {:?}",
    census.files
  );
  assert_eq!(
    census.gaps.len(),
    1,
    "the ungated `extern crate alloc` outside src/ was not reported"
  );
}

// --- instrument 1, member 4: `#[path]` on an inline module moves a subtree ---------------

#[test]
fn a_path_attribute_on_an_inline_module_moves_its_children() {
  let census = census(&[
    (
      "src/lib.rs",
      "#[path = \"../elsewhere\"]\nmod outer {\n  mod inner;\n}\n",
    ),
    ("elsewhere/inner.rs", "extern crate alloc;\n"),
  ])
  .expect("an inline `#[path]` redirects the directory its children resolve against");
  assert!(
    census.files.contains(&PathBuf::from("elsewhere/inner.rs")),
    "the subtree moved out of src/ was not read: {:?}",
    census.files
  );
  assert_eq!(census.gaps.len(), 1);
}

// --- instrument 1, member 5: a conditional `#[path]` ------------------------------------

#[test]
fn a_conditional_path_is_refused() {
  let refusal = census(&[(
    "src/lib.rs",
    "#[cfg_attr(feature = \"std\", path = \"../a.rs\")]\nmod either;\n",
  )])
  .expect_err("a file set that depends on the configuration is two file sets");
  assert!(refusal.contains("depend on"), "{refusal}");
}

// --- instrument 1, member 6: `include!` --------------------------------------------------

#[test]
fn an_include_is_refused() {
  let refusal = census(&[(
    "src/lib.rs",
    "include!(concat!(env!(\"OUT_DIR\"), \"/table.rs\"));\n",
  )])
  .expect_err("a spliced file is compiled into the crate and never named by the graph");
  assert!(refusal.contains("include"), "{refusal}");
}

// --- instrument 1, member 7: a macro whose expansion is not in the source ----------------

#[test]
fn a_macro_that_is_not_named_inert_is_refused() {
  let refusal = census(&[("src/lib.rs", "make_the_module!();\n")])
    .expect_err("a macro can expand to a `mod`, an `include!` or an `extern crate`");
  assert!(refusal.contains("expands to source"), "{refusal}");

  let defining = census(&[(
    "src/lib.rs",
    "macro_rules! quiet {\n  () => { extern crate alloc; };\n}\n",
  )])
  .expect_err("defining a macro is the same hazard as invoking one");
  assert!(defining.contains("expands to source"), "{defining}");
}

/// The exemptions are exactly the macros the library invokes, in both directions.
///
/// One direction is the census passing at all: a macro this list does not name stops it.
/// The other is this assertion, which is what keeps the list from outliving its reasons —
/// an exemption nothing needs is an exemption nobody re-reads.
#[test]
fn the_inert_macros_are_the_ones_this_crate_invokes() {
  let package = package();
  let library = census_of(&package.root, &read_from_disk, &package.world())
    .unwrap_or_else(|complaint| panic!("{complaint}"));
  let mut listed: Vec<&str> = INERT_MACROS.to_vec();
  listed.sort_unstable();
  assert_eq!(
    library.inert_invoked, listed,
    "the macros named inert and the macros the library invokes have diverged. A name with \
     nothing invoking it is an exemption that stopped being read; a macro missing from the \
     list would have refused the census rather than reaching here"
  );

  // ...and a qualified path is the same macro, which is how `std::format!` is written here.
  let qualified = census(&[(
    "src/lib.rs",
    "fn uses() {\n  let _ = std::format!(\"x\");\n  core::assert!(true);\n}\n",
  )])
  .expect("a macro written through its crate is the same macro");
  assert!(qualified.gaps.is_empty());
}

// --- instrument 1: the resolver is checked against the walk it replaced ------------------

#[test]
fn a_resolver_that_stops_early_is_visible_as_an_unreached_file() {
  // The real assertion lives in `nothing_outside_the_alloc_feature_reaches_alloc`; this
  // pins the shape it depends on, that a file no `mod` names is not in `files`.
  let census = census(&[
    ("src/lib.rs", "mod named;\n"),
    ("src/named.rs", ""),
    ("src/orphan.rs", "extern crate alloc;\n"),
  ])
  .expect("a crate whose graph does not reach every file on disk");
  assert!(!census.files.contains(&PathBuf::from("src/orphan.rs")));
  assert!(census.gaps.is_empty(), "an unreached file is not censused");
}

// --- instrument 2, member 1: the Cargo feature closure -----------------------------------

/// The finding: `#[cfg(feature = "std")] extern crate alloc;` was rejected although
/// `std = ["alloc", …]` means no build can enable `std` without `alloc`.
#[test]
fn a_feature_that_cargo_says_enables_alloc_implies_alloc() {
  let world = fixture_world();
  for gate in [
    "feature = \"std\"",
    "feature = \"tz\"",
    "all(unix, feature = \"std\")",
    "any(feature = \"alloc\", feature = \"std\")",
  ] {
    assert!(
      world
        .implies_the_alloc_feature(&parse_gate(gate))
        .expect("a gate this procedure decides"),
      "`{gate}` cannot hold in a build with `alloc` off, and the procedure said otherwise"
    );
  }
  for gate in [
    "feature = \"tz-static\"",
    "feature = \"jiff\"",
    "any(feature = \"alloc\", feature = \"tz-static\")",
  ] {
    assert!(
      !world
        .implies_the_alloc_feature(&parse_gate(gate))
        .expect("a gate this procedure decides"),
      "`{gate}` holds in builds with `alloc` off, and the procedure said otherwise"
    );
  }
}

// --- instrument 2, member 2: entries that are not this crate's features ------------------

#[test]
fn a_dependency_entry_ties_nothing_and_an_unknown_one_is_loud() {
  // `alloc = ["jiff?/alloc"]` and `jiff = ["dep:jiff"]` are the crate's own two shapes.
  let world = World::of(&BTreeMap::from([
    ("alloc".to_owned(), vec!["jiff?/alloc".to_owned()]),
    ("jiff".to_owned(), vec!["dep:jiff".to_owned()]),
    ("tz-static".to_owned(), vec!["jiff/static".to_owned()]),
  ]));
  assert!(
    !world
      .implies_the_alloc_feature(&parse_gate("feature = \"tz-static\""))
      .expect("a decided gate"),
    "a dependency entry tied `tz-static` to `alloc`"
  );
}

#[test]
#[should_panic(expected = "neither a feature of this crate nor a dependency entry")]
fn a_feature_entry_naming_nothing_is_refused() {
  World::of(&BTreeMap::from([(
    "std".to_owned(),
    vec!["allock".to_owned()],
  )]));
}

// --- instrument 2, members 3 and 4: values that cannot hold together ---------------------

#[test]
fn two_values_of_one_single_valued_key_cannot_both_hold() {
  let world = fixture_world();
  assert!(
    world
      .implies_the_alloc_feature(&parse_gate(
        "all(target_os = \"linux\", target_os = \"none\")"
      ))
      .expect("a decided gate"),
    "a gate no target satisfies gates nothing, and it was read as reaching alloc"
  );
  assert!(
    world
      .implies_the_alloc_feature(&parse_gate("all(unix, windows)"))
      .expect("a decided gate"),
    "no target is both unix and windows"
  );
  // ...and the keys that hold several values at once are left independent, which errs
  // towards a false alarm rather than a false pass.
  assert!(
    !world
      .implies_the_alloc_feature(&parse_gate(
        "all(target_feature = \"sse\", target_feature = \"sse2\")"
      ))
      .expect("a decided gate"),
    "a multi-valued key was treated as exclusive, which is the unsafe direction"
  );
}

// --- instrument 2, member 5: `cfg_attr` -------------------------------------------------

#[test]
fn a_cfg_attr_gates_the_item_on_the_predicate_it_applies_under() {
  // `#[cfg_attr(P, cfg(Q))]` leaves the item unconditional whenever `P` fails, so only a
  // `P` that cannot fail without `alloc` can carry the promise.
  let reaching = census(&[(
    "src/lib.rs",
    "#[cfg_attr(unix, cfg(feature = \"alloc\"))]\nextern crate alloc;\n",
  )])
  .expect("a `cfg_attr` this census decides");
  assert_eq!(
    reaching.gaps.len(),
    1,
    "on a non-unix target the item is unconditional, and that was missed"
  );

  let gated = census(&[(
    "src/lib.rs",
    "#[cfg(feature = \"alloc\")]\n#[cfg_attr(unix, cfg(target_os = \"linux\"))]\n\
     extern crate alloc;\n",
  )])
  .expect("a `cfg_attr` this census decides");
  assert!(
    gated.gaps.is_empty(),
    "a `cfg_attr` narrowing an already-implying gate was read as widening it: {:?}",
    gated.gaps
  );
}

// --- instrument 2: the enclosing module's gate ------------------------------------------

#[test]
fn a_module_gate_reaches_the_items_inside_it() {
  let gated = census(&[
    ("src/lib.rs", "#[cfg(feature = \"alloc\")]\nmod heap;\n"),
    ("src/heap.rs", "extern crate alloc;\n"),
  ])
  .expect("a gated module gates what is in it");
  assert!(
    gated.gaps.is_empty(),
    "the module's own `#[cfg]` was not carried to its items: {:?}",
    gated.gaps
  );

  let ungated = census(&[
    ("src/lib.rs", "#[cfg(feature = \"tz-static\")]\nmod heap;\n"),
    ("src/heap.rs", "extern crate alloc;\n"),
  ])
  .expect("a gated module gates what is in it");
  assert_eq!(ungated.gaps.len(), 1);
}

// --- instrument 2: every bypass the line scan of round one let through -------------------

#[test]
fn the_alloc_census_rejects_every_known_bypass_of_the_line_scan() {
  for (bypass, source) in [
    (
      "a disjunction, which lets `tz-static` reach alloc",
      "#[cfg(any(feature = \"alloc\", feature = \"tz-static\"))]\nextern crate alloc;\n",
    ),
    (
      "the same disjunction on one line, which the line scan did not even see",
      "#[cfg(any(feature = \"alloc\", feature = \"tz-static\"))] extern crate alloc;\n",
    ),
    (
      "a negation, which means the opposite and contains the same literal",
      "#[cfg(not(feature = \"alloc\"))]\nextern crate alloc;\n",
    ),
    (
      "a comment above the item",
      "// gated by feature = \"alloc\" further up\nextern crate alloc;\n",
    ),
    (
      "a comment on the item's own line",
      "extern crate alloc; // feature = \"alloc\" is on\n",
    ),
    (
      "a raw string literal above the item",
      "const G: &str = r#\"feature = \"alloc\"\"#;\nextern crate alloc;\n",
    ),
    (
      "a visibility, which the line scan's prefix match stopped seeing",
      "pub extern crate alloc;\n",
    ),
    (
      "another tier's feature, on one line",
      "#[cfg(feature = \"tz-static\")] extern crate alloc;\n",
    ),
    (
      "an item inside a function body",
      "fn takes_a_heap() {\n  extern crate alloc;\n}\n",
    ),
    (
      "an item inside an inline module",
      "mod inner {\n  extern crate alloc;\n}\n",
    ),
  ] {
    let census = census(&[("src/lib.rs", source)])
      .unwrap_or_else(|complaint| panic!("{bypass}: the census could not read it: {complaint}"));
    assert_eq!(
      census.gaps.len(),
      1,
      "{bypass}: the census should have found exactly one gap in\n{source}but found {:?}",
      census.gaps
    );
  }
}

#[test]
fn the_alloc_census_accepts_a_gate_that_implies_the_feature() {
  for source in [
    "#[cfg(feature = \"alloc\")]\nextern crate alloc;\n",
    "#[cfg(feature=\"alloc\")] extern crate alloc;\n",
    "#[cfg(feature = \"alloc\")]\n#[allow(unused_imports)]\nextern crate alloc;\n",
    "#[cfg(feature = \"alloc\")]\npub extern crate alloc as heap;\n",
    "#[cfg(all(feature = \"alloc\", target_os = \"none\"))]\nextern crate alloc;\n",
    "#[cfg(feature = \"alloc\")]\n#[cfg(target_os = \"none\")]\nextern crate alloc;\n",
    "#[cfg(any(all(unix, feature = \"alloc\"), all(windows, feature = \"alloc\")))]\n\
     extern crate alloc;\n",
    // Not gated by the feature, but gated by nothing at all, so the item exists nowhere.
    "#[cfg(false)]\nextern crate alloc;\n",
    // The item is not `alloc`, and a prefix match would say it was.
    "extern crate alloc_shim;\n",
    // Prose about the item is not the item. src/lib.rs has exactly this today.
    "// The `extern crate alloc;` that would go with them arrives with its first user.\n",
  ] {
    let census = census(&[("src/lib.rs", source)])
      .unwrap_or_else(|complaint| panic!("the census could not read {source}: {complaint}"));
    assert!(
      census.gaps.is_empty(),
      "this source gates `alloc` behind its feature and the census says otherwise:\n{source}\
       {:?}",
      census.gaps
    );
  }
}

fn parse_gate(text: &str) -> Cfg {
  syn::parse_str::<Cfg>(text).expect("a cfg predicate this file wrote")
}

// --- instrument 3: the workflow --------------------------------------------------------

/// `base` with `from` replaced by `to`, and a loud failure if the anchor is gone.
///
/// The fixtures below are mutations of the real file rather than miniatures of it, so that
/// each one asks the census its question in the setting it actually runs in. The cost is
/// that reformatting the `no-std` job breaks them; the assertion is what turns that into a
/// sentence instead of a fixture that quietly stops planting anything.
fn planted(base: &str, from: &str, to: &str) -> String {
  assert!(
    base.contains(from),
    "a fixture below anchors on text that is no longer in ci.yml, so it plants nothing:\n\
     {from}"
  );
  base.replace(from, to)
}

/// The control. Every fixture is this file with one thing changed, so a red here would
/// make all of them meaningless.
#[test]
fn the_workflow_census_accepts_the_workflow_as_it_stands() {
  assert_eq!(every_hostless_tier_has_a_cell(WORKFLOW), Ok(()));
}

#[test]
fn the_workflow_census_rejects_a_tier_named_only_in_a_comment() {
  let fixture = planted(
    WORKFLOW,
    "features: '--features tz-static'",
    "features: ''  # was --features tz-static",
  );
  let complaint = every_hostless_tier_has_a_cell(&fixture)
    .expect_err("a tier whose flag survives only in a comment is not built by anything");
  assert!(
    complaint.contains("`tz-static` claims to need no host"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );
}

#[test]
fn the_workflow_census_rejects_a_tier_left_in_a_matrix_key_nothing_reads() {
  let fixture = planted(
    WORKFLOW,
    "features: '--features tz-static'",
    "features: ''\n            legacy-features: '--features tz-static'",
  );
  let complaint = every_hostless_tier_has_a_cell(&fixture)
    .expect_err("a matrix key no step interpolates builds nothing");
  assert!(
    complaint.contains("`legacy-features`"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );
}

/// The other half of "the rows this file reads are the cells CI runs": an axis multiplies
/// them and an `exclude` removes them, and neither is modelled here.
#[test]
fn the_workflow_census_rejects_a_matrix_that_is_more_than_its_include_rows() {
  for (key, planted_line) in [
    (
      "axis",
      "      matrix:\n        rust: [stable, beta]\n        include:",
    ),
    (
      "exclude",
      "      matrix:\n        exclude:\n          - tier: tz-static\n        include:",
    ),
  ] {
    let fixture = planted(WORKFLOW, "      matrix:\n        include:", planted_line);
    let complaint = every_hostless_tier_has_a_cell(&fixture)
      .expect_err("a matrix that is more than its include rows runs other cells");
    assert!(
      complaint.contains("beside `include`"),
      "rejected, but not for the `{key}` the fixture plants: {complaint}"
    );
  }
}

#[test]
fn the_workflow_census_rejects_a_cell_pointed_at_a_host_target() {
  const HOST: &str = "target: x86_64-unknown-linux-gnu\n            features: '--features \
                      tz-static'";
  let fixture = planted(
    WORKFLOW,
    "target: thumbv7em-none-eabi\n            features: '--features tz-static'",
    HOST,
  );
  let fixture = planted(
    &fixture,
    "target: thumbv6m-none-eabi\n            features: '--features tz-static'",
    HOST,
  );
  let complaint = every_hostless_tier_has_a_cell(&fixture)
    .expect_err("a host target has the std the tier disclaims, so it cannot refuse anything");
  assert!(
    complaint.contains("`tz-static` claims to need no host"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );
}

#[test]
fn the_workflow_census_rejects_cells_the_cargo_command_never_reads() {
  let fixture = planted(
    WORKFLOW,
    "run: cargo check --lib --no-default-features ${{ matrix.features }} --target \
     ${{ matrix.target }}",
    "run: cargo check --lib --no-default-features --target ${{ matrix.target }}",
  );
  let complaint = every_hostless_tier_has_a_cell(&fixture)
    .expect_err("cells the command does not interpolate build the same thing seven times");
  assert!(
    complaint.contains("${{matrix.features}}"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );
}

/// Every key under the job that can undo a red cargo check, one fixture per member.
#[test]
fn the_workflow_census_rejects_every_key_that_undoes_a_failure() {
  for (key, planted_line) in [
    ("if", "        if: false\n        run: cargo check --lib"),
    (
      "continue-on-error",
      "        continue-on-error: true\n        run: cargo check --lib",
    ),
    (
      "shell",
      "        shell: bash -c \"$@ || true\"\n        run: cargo check --lib",
    ),
    (
      "working-directory",
      "        working-directory: ../elsewhere\n        run: cargo check --lib",
    ),
  ] {
    let fixture = planted(WORKFLOW, "        run: cargo check --lib", planted_line);
    let complaint = every_hostless_tier_has_a_cell(&fixture)
      .expect_err("a key that undoes a failure leaves the cell unable to state anything");
    assert!(
      complaint.contains(key),
      "rejected, but not for the `{key}` the fixture plants: {complaint}"
    );
  }
}

/// Every shell operator that makes the step's exit status something other than the cargo
/// check's, one fixture per member of [`SHELL_OPERATORS`] — and one for the same operator
/// arriving through a value the template interpolates, since the command as executed is
/// both.
#[test]
fn the_workflow_census_rejects_every_shell_operator_in_the_command() {
  const RUN: &str = "        run: cargo check --lib --no-default-features ${{ matrix.features \
                     }} --target ${{ matrix.target }}";
  for (operator, tail) in [
    ('|', "| cat"),
    ('&', "& wait"),
    (';', "; true"),
    ('<', "< log"),
    ('>', "> log"),
    ('(', "(true)"),
    (')', "x)"),
    ('{', "{ true"),
    ('}', "true }"),
    ('$', "$HOME"),
    ('`', "`true`"),
    ('\\', "\\ true"),
    ('"', "\"true\""),
    ('\'', "'true'"),
  ] {
    let fixture = planted(WORKFLOW, RUN, &format!("{RUN} {tail}"));
    let complaint = every_hostless_tier_has_a_cell(&fixture)
      .expect_err("a second command, or a substitution, can answer for the cargo check");
    assert!(
      complaint.contains(&format!("contains `{operator}`")),
      "rejected, but not for the `{operator}` the fixture plants: {complaint}"
    );
  }

  // The two line breaks are one member each and plant the same way: a `run:` that is a
  // block scalar is more than one command whatever the commands are.
  let block = planted(
    WORKFLOW,
    RUN,
    "        run: |\n          cargo check --lib --no-default-features ${{ matrix.features }} \
     --target ${{ matrix.target }}\n          true",
  );
  let complaint = every_hostless_tier_has_a_cell(&block)
    .expect_err("a block scalar holds a script, and the last line answers for it");
  assert!(complaint.contains("exit status"), "{complaint}");
}

/// The same operator, arriving in a value rather than in the template. Both of the values
/// this command interpolates get a fixture, because the two are refused by different parts
/// of this file and a fixture that does not say which proves neither.
#[test]
fn the_workflow_census_rejects_an_operator_arriving_through_a_matrix_value() {
  // `target` is passed through as written, so the scan of the command as executed is what
  // catches it.
  let through_target = planted(
    WORKFLOW,
    "target: thumbv6m-none-eabi\n            features: ''",
    "target: 'thumbv6m-none-eabi || true'\n            features: ''",
  );
  let complaint = every_hostless_tier_has_a_cell(&through_target)
    .expect_err("the command as executed is the template plus its values");
  assert!(
    complaint.contains("interpolates"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );

  // `features` is read as a flag first, so the shape refusal gets there first — which is
  // the same answer reached one step earlier, and the fixture says so rather than reading
  // a rejection as agreement.
  let through_features = planted(
    WORKFLOW,
    "features: '--features tz-static'",
    "features: '--features tz-static || true'",
  );
  let complaint = every_hostless_tier_has_a_cell(&through_features)
    .expect_err("a flag shape this file does not model is not read as building a tier");
  assert!(
    complaint.contains("which this test does not model"),
    "rejected, but not for the reason the fixture plants: {complaint}"
  );
}

/// The two that look like they undo a failure and do not, decided rather than refused.
#[test]
fn the_workflow_census_allows_what_cannot_undo_a_failure() {
  let with_timeout = planted(
    WORKFLOW,
    "  no-std:\n    name: no-std (${{ matrix.tier }} / ${{ matrix.target }})\n",
    "  no-std:\n    name: no-std (${{ matrix.tier }} / ${{ matrix.target }})\n    \
     timeout-minutes: 20\n",
  );
  assert_eq!(
    every_hostless_tier_has_a_cell(&with_timeout),
    Ok(()),
    "a timeout fails the step; it cannot forgive one"
  );
  assert!(
    WORKFLOW.contains("fail-fast: false"),
    "`fail-fast` is decided as harmless in the list above, and it cancels other cells \
     rather than forgiving this one — but the workflow it was decided against no longer \
     sets it"
  );
}
