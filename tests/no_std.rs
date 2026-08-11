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
//! Both of those are questions about structure, and both were first written as scans of
//! the text, which cannot express either one.
//!
//! Over the workflow, a scan can say "the bytes `--features tz-static` occur somewhere
//! between the `no-std:` line and the next job". A comment satisfies that, so does a
//! matrix key no step reads, and so does a row aimed at a host target — none of which
//! builds anything. What the job actually builds is the `features` of each `include` row,
//! crossed with that row's `target`, and only if the cargo command interpolates them; that
//! is a question for the YAML tree, so the tree is what [`every_hostless_tier_has_a_cell`]
//! reads.
//!
//! Over the source, a scan can say "this line, or the one above it, contains
//! `feature = \"alloc\"`". The requirement is that the item's `#[cfg]` *implies* the
//! `alloc` feature, and mentioning is not implying: `any(feature = "alloc", feature =
//! "tz-static")` mentions it while letting `tz-static` reach `alloc`, and
//! `not(feature = "alloc")` mentions it while meaning the exact opposite. A scan also
//! cannot see which item an attribute is attached to, or tell code from a comment or a
//! string — so a comment above the item passes it, and `#[cfg(...)] extern crate alloc;`
//! written on one line is not even noticed. [`nothing_outside_the_alloc_feature_reaches_alloc`]
//! therefore parses each file as Rust and decides the implication over the predicate's
//! truth assignments.
//!
//! Three things it does not see, stated rather than left to be discovered. A `#[cfg]` on
//! an enclosing `mod` is not counted, which can only cost a false alarm: a gate that
//! already implies `alloc` still implies it under any further conjunct. A file spliced in
//! by `include!` is compiled and never walked, so the census refuses such a file outright
//! rather than reporting the rest of it clean. An `extern crate alloc` produced by a macro
//! expansion is an item in no file, and that one is a real gap — narrow only because this
//! crate declares no macro, which `grep -r 'macro_rules!' src/` still answers 0 to.

use std::{
  fmt,
  path::{Path, PathBuf},
};

use syn::{
  Meta, Token,
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
    match alloc_reached_without_its_feature(&source) {
      Ok(gaps) => assert!(
        gaps.is_empty(),
        "{} reaches `alloc` without the `alloc` feature: {}. The crate's description, its \
         `no-std::no-alloc` category and the default row of the README's feature table \
         all promise that the default tier allocates nothing",
        path.display(),
        gaps.join("; ")
      ),
      Err(complaint) => panic!("{}: {complaint}", path.display()),
    }
  }
}

// ---------------------------------------------------------------------------------------
// The alloc census, and the bypasses it is written against.
// ---------------------------------------------------------------------------------------

/// A `#[cfg]` predicate, in the shape `cfg` itself defines it.
///
/// Held as a tree rather than as text because the question asked of it —
/// [`implies_the_alloc_feature`] — is about what the predicate admits, and `all`, `any`
/// and `not` differ in exactly that while differing in text by three letters.
enum Cfg {
  /// `cfg(true)` and `cfg(false)`, which say nothing about a feature and everything about
  /// whether the item exists.
  Const(bool),
  /// A leaf: `unix`, `feature = "alloc"`, `target_os = "none"`.
  Atom(String),
  /// A conjunction. Also how an item's several `#[cfg]` attributes combine, and how an
  /// item with none is represented — an empty `all()` is true, which is the whole of why
  /// an ungated item fails this census.
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

impl fmt::Display for Cfg {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// Whether every configuration `gate` admits is one with the `alloc` feature on.
///
/// Decided rather than recognised. The predicate's atoms are enumerated over every truth
/// assignment and the implication is the absence of a counterexample — an assignment where
/// the item exists and the feature is off. That is what makes the answer indifferent to
/// how the predicate is written: `any(feature = "alloc", feature = "tz-static")` has a
/// counterexample and `not(feature = "alloc")` is a counterexample everywhere it holds,
/// while both contain the literal a text scan looks for. A predicate no configuration
/// satisfies — `all(unix, not(unix))`, `false` — implies this vacuously, and correctly:
/// the item it gates is compiled nowhere.
fn implies_the_alloc_feature(gate: &Cfg) -> Result<bool, String> {
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

  let alloc_at = atoms
    .iter()
    .position(|atom| atom == ALLOC)
    .ok_or("the alloc atom is seeded above and cannot be missing")?;

  for assignment in 0u32..(1u32 << atoms.len()) {
    let set = |atom: &str| match atoms.iter().position(|known| known == atom) {
      Some(at) => assignment & (1 << at) != 0,
      None => false,
    };
    if gate.holds(&set) && assignment & (1 << alloc_at) == 0 {
      return Ok(false);
    }
  }
  Ok(true)
}

/// Every `extern crate alloc` in `source`, wherever it sits.
///
/// A visitor rather than a walk of the top-level items, so that one inside a module, a
/// function body or a `const` block is found on the same terms as one at the crate root.
#[derive(Default)]
struct AllocExterns<'ast> {
  items: Vec<&'ast syn::ItemExternCrate>,
  /// Whether the file splices in one this census never opens.
  ///
  /// The walk reads `src/**/*.rs`, and the promise is about everything the crate compiles.
  /// Those are the same set only while nothing is `include!`d — this crate has a build
  /// script, and a generated file under `OUT_DIR` would be compiled without ever being
  /// walked. There is no such splice today, and this is what stops the first one landing
  /// quietly.
  splices: bool,
}

impl<'ast> Visit<'ast> for AllocExterns<'ast> {
  fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
    if item.ident == "alloc" {
      self.items.push(item);
    }
  }

  fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
    if invocation.path.is_ident("include") {
      self.splices = true;
    }
    syn::visit::visit_macro(self, invocation);
  }
}

/// Every `extern crate alloc` in `source` whose own `#[cfg]`s admit a build with the
/// `alloc` feature off, described.
///
/// `Err` is the instrument failing rather than the source: a file that does not parse, or
/// a `#[cfg]` written in a form this does not model. Both are louder than a pass, which is
/// the point — a census that cannot read its input must not report a clean one.
fn alloc_reached_without_its_feature(source: &str) -> Result<Vec<String>, String> {
  let file = syn::parse_file(source).map_err(|error| format!("does not parse as Rust: {error}"))?;
  let mut found = AllocExterns::default();
  found.visit_file(&file);
  if found.splices {
    return Err(
      "this file `include!`s another, which the walk of src/ does not read. What this \
       census reaches and what the crate compiles have to be the same set of items, and \
       from here they are not"
        .to_owned(),
    );
  }

  let mut gaps = Vec::new();
  for item in found.items {
    let mut gates = Vec::new();
    for attribute in &item.attrs {
      if !attribute.path().is_ident("cfg") {
        continue;
      }
      let Meta::List(list) = &attribute.meta else {
        return Err("a `#[cfg]` that is not a predicate list".to_owned());
      };
      gates.push(
        syn::parse2::<Cfg>(list.tokens.clone())
          .map_err(|error| format!("a `#[cfg]` this census cannot read: {error}"))?,
      );
    }

    let named = match &item.rename {
      Some((_, alias)) => format!("`extern crate alloc as {alias}`"),
      None => "`extern crate alloc`".to_owned(),
    };
    if gates.is_empty() {
      gaps.push(format!("{named} carries no `#[cfg]` at all"));
      continue;
    }

    // Several `#[cfg]`s on one item are a conjunction; one is itself, and reads back to
    // the reader as the source wrote it.
    let gate = match <[Cfg; 1]>::try_from(gates) {
      Ok([only]) => only,
      Err(several) => Cfg::All(several),
    };
    if !implies_the_alloc_feature(&gate)? {
      gaps.push(format!(
        "{named} is gated by `{gate}`, which admits a build with the `alloc` feature off"
      ));
    }
  }
  Ok(gaps)
}

/// The forms that do imply the feature, including the two the line scan this replaced got
/// wrong: an attribute between the gate and the item, which it rejected, and a visibility
/// on the item, which it stopped seeing altogether.
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
    assert_eq!(
      alloc_reached_without_its_feature(source),
      Ok(Vec::new()),
      "this source gates `alloc` behind its feature and the census says otherwise:\n{source}"
    );
  }
}

/// A file the walk of src/ never opens is not covered by anything above it, so the census
/// says so rather than reporting a clean sweep of the files it happens to hold.
#[test]
fn the_alloc_census_refuses_a_file_that_splices_in_one_it_cannot_read() {
  let complaint =
    alloc_reached_without_its_feature("include!(concat!(env!(\"OUT_DIR\"), \"/table.rs\"));\n")
      .expect_err("a spliced file is compiled into the crate and never walked");
  assert!(
    complaint.contains("`include!`s another"),
    "refused, but not for the reason the fixture plants: {complaint}"
  );
}

/// Every way found so far of mentioning the feature, or of hiding the item, that the line
/// scan this replaced let through. Each of these passed it.
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
    let gaps = alloc_reached_without_its_feature(source)
      .unwrap_or_else(|complaint| panic!("{bypass}: the census could not read it: {complaint}"));
    assert_eq!(
      gaps.len(),
      1,
      "{bypass}: the census should have found exactly one gap in\n{source}but found {gaps:?}"
    );
  }
}

// ---------------------------------------------------------------------------------------
// The workflow census, and the bypasses it is written against.
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

/// Whether every tier that claims to need no host is built by a cell that could refuse it.
///
/// Reads the workflow as YAML because every part of the question is structural: a feature
/// is built only if it is in the `features` of an `include` row, only if that row's
/// `target` is bare metal, only if the cargo command interpolates the row, and only if the
/// job lets the result fail it. Text answers none of those — a comment satisfies a scan, so
/// does a matrix key no step reads, so does a row aimed at a host, and so does a cell the
/// job forgives. Each has a fixture below.
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

  // A cell that cannot go red is not a gate, and every question below is asked as though
  // each one could refuse its tier. Neither key is in this job today, so neither can
  // arrive without arriving here first.
  for escape in ["continue-on-error", "if"] {
    if names_a_key(job, escape) {
      return Err(format!(
        "the `no-std` job carries a `{escape}`. A cell that can be skipped, or that can \
         fail without failing the job, builds a tier without gating it — which is the \
         state this whole file exists to make impossible"
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
  let command = interpolations_normalised(
    commands
      .next()
      .ok_or("no step of the `no-std` job runs `cargo check`")?,
  );
  if commands.next().is_some() {
    return Err(
      "more than one step of the `no-std` job runs `cargo check`, so which one a cell \
       reaches is no longer something this test can say"
        .to_owned(),
    );
  }
  for required in [
    "--lib",
    "--no-default-features",
    "${{matrix.features}}",
    "--target ${{matrix.target}}",
  ] {
    if !command.contains(required) {
      return Err(format!(
        "the `no-std` job's cargo command does not pass `{required}`, so its cells no \
         longer say what this test reads them as saying: `{command}`"
      ));
    }
  }

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
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find("${{") {
    out.push_str(&rest[..open]);
    let after = &rest[open..];
    match after.find("}}") {
      Some(close) => {
        out.extend(after[..close + 2].chars().filter(|c| !c.is_whitespace()));
        rest = &after[close + 2..];
      }
      None => {
        out.push_str(after);
        return out;
      }
    }
  }
  out.push_str(rest);
  out
}

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

/// Not one of the three bypasses the review named — a cell can also be present, aimed at a
/// bare-metal target and consumed by the command, and still not be a gate, because the job
/// forgives it.
#[test]
fn the_workflow_census_rejects_a_cell_that_cannot_go_red() {
  let fixture = planted(
    WORKFLOW,
    "        run: cargo check --lib",
    "        continue-on-error: true\n        run: cargo check --lib",
  );
  let complaint = every_hostless_tier_has_a_cell(&fixture)
    .expect_err("a cell whose failure is forgiven states nothing about its tier");
  assert!(
    complaint.contains("`continue-on-error`"),
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
