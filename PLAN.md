# PLAN: making FRIEDA a top-notch general-purpose automata crate

Goal: a published, documented, well-tested Rust crate for finite- and omega-word automata (DFA, NFA, Moore/Mealy, DBA/NBA, DPA/NPA, Rabin, Muller, ...) with HOA interoperability, with learning algorithms as an optional layer.

Work is split into phases. Each phase leaves the repo green (`./check` passes) and is independently mergeable. Do them in order; items inside a phase can be tackled in any order.

## Current state (assessment)

Strengths:
- Builds cleanly and ~145 tests pass. Solid algorithmic content: SCCs, products, subset construction, partition-refinement minimization, congruences, HOA I/O, parity/Rabin/Muller/Buchi, L*, sprout/DPAInf.
- Sensible layering (`hoars` and `automata-core` as leaves) and a capability-trait design (`Deterministic`, `Pointed`, `Sproutable`, `Shrinkable`).
- `deny(missing_docs)` is on for `automata`, CI is in place.

Problems found:

### Packaging and structure
1. **Nothing is published or publishable as-is.** README says "crates have not yet been published"; users must clone and use path dependencies.
2. **Crate split is arbitrary.** `automata` re-exports all of `automata-core` as `automata::core`, so `automata-core` is never used on its own. Two crates make users learn two namespaces for one concept, and `Alphabet`/`Word`/`Color` are inseparable from `TransitionSystem` in practice. `automata-core` also declares an unused optional `hoars` dependency with no feature.
3. **Inconsistent metadata.** `automata` is 0.0.4, `automata-core` 0.1.0, `hoars` 0.2.1 (while dependents require `0.2.0`), `automata-learning` has no metadata at all. `license = "MIT or Apache-2.0"` is not valid SPDX (`OR`), and only an MIT `LICENSE.md` exists. `repository`/`homepage` point to `leonbohn/automata`, not this repo (and `hoars` points to a subpath of it). Copyright year and name capitalization in the license are off.
4. **Misplaced dependencies.** `test-log` and `tracing-subscriber` are normal dependencies of library crates (should be dev-dependencies / left to the application). `owo-colors` is unconditional. Heavy rendering stack (`resvg`, `usvg` with system fonts, `layout-rs`, `open`, `base64`) is in the `default` features, which makes the default build heavy and unsuitable for a library.
5. **Stale auxiliary projects.** `bench/` has `edition = "2021"`, a wrong path (`../automata` rather than `../crates/automata`), and `itertools 0.15` against the workspace's 0.14; `perf` is tied to specific git hashes. `python/` is excluded from the workspace, not CI-built (the maturin workflow is generated boilerplate), and only wraps a DFA. `benchmarks.csv`, `check`, `perf`, `crates/automata-learning/verify` (a duplicate of `check`), and `crates/hoars/hoa/` sample files are scattered; `examples/hoa` is at the root.
6. **`Cargo.lock` is gitignored** though the workspace contains binaries. Also `rust-toolchain`/MSRV is not declared.
7. **Repo/crate mismatch.** The repo is "frieda" but the main crate is `automata`, which the maintainer already owns on crates.io (latest published: 0.0.4, whose metadata points at `github.com/leonbohn/automata`). Users looking for the crate land in a multi-crate research workspace instead of a repo that is about that crate.

### API and design
8. **Heavy type-level machinery.** `Automaton<A, Z, Q, C, D, const OMEGA, const DET>` has seven parameters, with two const bools that duplicate information carried by `Z` and `D`. `Semantics<T, const OMEGA: bool>` forces the same split. Error messages and docs become hard to read; `Automaton` derives `Copy`.
9. **Many public items are undocumented in practice.** 34 `allow(missing_docs)`, `automata-learning` disables docs and unused warnings entirely (18 `allow(unused)`), `families` is fully exempt.
10. **Panicking API.** ~317 `unwrap`/`expect` and ~66 `panic!` calls in library code (for example `Color::reduce` on an empty iterator, state lookups that panic on missing indices). 11 `allow(clippy::result_large_err)` signals large error types.
11. **Unclear NFA story.** Nondeterminism is a const bool on the TS, but there is no first-class NFA/NBA surface: no determinization to a public `DFA` entry point documented, no epsilon handling, no regular expression support.
12. **Missing core operations for a general-purpose crate**: language equivalence and inclusion, emptiness for every acceptance type, complement for all deterministic types, Buchi/Rabin/Muller to parity conversions, Safra-style determinization, DFA minimization via Hopcroft/Brzozowski as documented entry points, union/intersection for omega automata, regex and LTL-free language builders, and trace/witness extraction (accepting word, counterexample).
13. **Index types leak.** `StateIndex` is generic in the trait, but most helpers assume `usize`-like indices (`IndexType`, `ScalarIndexType`), and `petgraph` and `implementations` features are sparsely exercised.
14. **`ts.rs` has an empty `mod tests {}`** and `automata-learning/src/lib.rs` has an empty one; the root `lib.rs` hosts test helpers (`wiki_dfa`).

### Testing and quality
15. **No integration tests, examples, or benches** in any crate (`tests/`, `examples/`, `benches/` directories are absent). Tests are only in-module unit tests; `~14` are `#[ignore]`d, and the README has 7 `ignore`d code blocks.
16. **No property-based or differential testing.** Automata operations are ideal for proptest/quickcheck (for example `minimize(A) ≡ A`, `A ∪ ¬A` accepts everything, round trip through HOA), and for cross-checking against Spot via HOA.
17. **CI gaps.** `rust.yml` only builds on Windows and macOS (not Linux), tests only on Linux stable, has no feature-matrix (`--no-default-features`, `--all-features`), no MSRV job, no docs deployment, no coverage, no `cargo deny`/`audit`, and no release automation. `caching.yml` duplicates work. `cargo build` rather than `cargo check --workspace --all-targets`.

### Documentation
18. **README is weak.** The root README is a stub pointing at crate READMEs; `automata` README relies on `ignore`d snippets; `automata-learning` README is ~10 lines with a sentence fragment; `hoars` README is 27 lines. No feature overview table, no comparison to alternatives (Spot, `automata`-style crates, `regex-automata`), no guide, no architecture docs, no CHANGELOG, no CONTRIBUTING.
19. **Rustdoc has little narrative.** Trait docs explain edges versus transitions well, but there is no crate-level tour, and no runnable examples on most public functions.

## Phases

### Phase 0: hygiene (status: mostly done)
- [x] Commit `Cargo.lock` (removed from `.gitignore`).
- [x] License: set to `MIT` (matches the only license file) via `[workspace.package]`; fixed name/year in `LICENSE.md`. Switching to `MIT OR Apache-2.0` later needs an Apache `LICENSE-APACHE` file and a decision from the maintainer.
- [x] Shared metadata (edition, license, repository, homepage, authors) moved to `[workspace.package]` and inherited; binaries set `publish = false`; `automata-learning` got a description.
- [x] `test-log` is now a dev-dependency of `automata`; `tracing-subscriber` removed from all library crates (still used by the binaries). `owo-colors` stays: it is used in library output code, not only tests.
- [x] Dropped the unused `hoars` dependency from `automata-core`; `automata` requires `hoars 0.2.1`.
- [x] Removed `bench/`, `perf`, `benchmarks.csv`, the `commit_N` features and the duplicate `automata-learning/verify`; criterion benchmarks come in Phase 4. `check` moved to `scripts/check`, HOA samples consolidated in `crates/hoars/hoa/`.
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean (one test lint fixed); `scripts/check` now runs that.
- [ ] `rust-version` / MSRV and `rust-toolchain.toml`: needs a toolchain manager to verify; do it in the CI rewrite (Phase 4) with `cargo-msrv`.
- [ ] Triage the ~40 `TODO`/`FIXME`/`todo!()` markers: fix or turn into issues.
- [ ] Decide fate of `python/` (see Phase 6); it is untouched so far.

### Phase 1: split into focused repositories and merge the core
Decision (recommended): **the `automata` crate gets its own repository, with `automata-core` merged into it.** The crate name `automata` is already owned on crates.io, so the new release is `automata 0.1.0` (a breaking change relative to 0.0.4, which is fine for 0.x).

Target repositories:

| Repo | Contents | Published as |
|---|---|---|
| `leonbohn/automata` (single-crate repo, crate at the root) | alphabets, words, transition systems, automata, algorithms, HOA bridge, `examples/`, `benches/`, `tests/`, mdBook guide | `automata` |
| `leonbohn/hoars` | current `crates/hoars` (own audience: anyone parsing HOA) | `hoars` |
| `leonbohn/frieda` (this repo, kept as research umbrella) | `automata-learning`, CLI binaries, `python/`, benchmarks and learning-task data | `automata-learning` (optional), binaries not published |

Steps:
- [ ] Verify the state of the existing `leonbohn/automata` repo (it is referenced by the published 0.0.4 and by `bench/`). Either reuse it (tag the old history as `legacy-0.0.x`, then replace the content) or create a fresh repo and update the crates.io `repository` link on the next release.
- [ ] Extract with history preserved (`git filter-repo --path crates/automata --path crates/automata-core ...` into a fresh clone), then flatten into one crate: move `alphabet`, `word`, `math`, `dag`, `Color`/`Lattice`/`Void`/`Show` into `automata` as top-level modules (`automata::alphabet`, `automata::word`, ...) and drop the `automata::core` indirection. Delete the `automata-core` crate; do not publish it.
- [ ] Extract `hoars` the same way. Until both are released, develop with a `[patch.crates-io]` or path override; afterwards `automata` depends on the published `hoars` version only (single, consistent requirement).
- [ ] In this repo, keep `automata-learning`, the three binaries (merged into one CLI, see Phase 6), `python/` and `bench/`. Replace path dependencies on `crates/automata` with `automata = "0.1"` from crates.io (or a git dependency while the API still moves). Make sure `automata-learning` only uses the public API of `automata` (audit for anything that relies on internals).
- [ ] Decide separately whether learning is ever split into its own repo (not needed now).
- [ ] In the new `automata` repo: copy over CI, `check`, dependabot, license files; fix `repository`/`homepage`/`documentation` fields; set up docs.rs metadata (`[package.metadata.docs.rs] all-features = true`).
- [ ] Split heavy features: `default = ["hoa"]`; `render`, `graphviz`, `random`, `petgraph` become opt-in. Keep `dot` text output always available since it needs no dependencies.
- [ ] Cross-repo cost to manage: API changes in `automata` now require a release (or git rev bump) before `automata-learning` can use them. Mitigate by freezing the learning crate until `automata`'s API redesign (Phase 2) lands, then porting it once.

Why not keep one workspace repo: a single-crate repo gives the crate its own issues, releases, CI badges, README front page and docs identity; the learning/research code has a different audience and release cadence and would otherwise block or clutter the library. Why not split everything: `automata-core` has no standalone users, so it would only add a second crate to version.

### Phase 2: API redesign
- [ ] Reduce `Automaton`'s parameters. Options to evaluate with a spike: (a) make acceptance condition a trait with an associated "word kind", removing `OMEGA`; (b) make determinism a property of the TS implementation instead of a const bool, removing `DET`. Provide concise aliases (`DFA`, `NFA`, `DBA`, `NBA`, `DPA`, `NPA`, `DRA`, `DMA`, `Moore`, `Mealy`).
- [ ] Introduce a `prelude` module with the 15 or so most used items.
- [ ] Replace panics in public API with `Result`/`Option` and a single crate `Error` type (`thiserror`); box or shrink large error variants and drop the `result_large_err` allows.
- [ ] Add first-class `NFA`/`NBA` with a documented `.determinize()`; add epsilon-NFA support if desired.
- [ ] Make the `Automaton` no longer `Copy`; audit derives.
- [ ] Review index-type genericity: either support non-`usize` indices properly or simplify to a `StateId` newtype.
- [ ] `#[non_exhaustive]` on public enums and error types; sealed traits where extension is not intended.

### Phase 3: algorithm completeness
Prioritize by general utility:
1. Boolean operations on every acceptance type where closure exists; complement for deterministic parity, Rabin, Muller.
2. Emptiness and witness extraction (accepting finite word / lasso) for all conditions.
3. Equivalence and inclusion checks with counterexamples.
4. Minimization: Hopcroft DFA (benchmark against existing partition refinement), Moore/Mealy minimization, DPA minimization/priority reduction.
5. Acceptance-condition conversions (Buchi to parity, Rabin/Muller to parity, parity normalization), and Safra determinization for NBA.
6. Regex parsing to NFA, and simple LTL-free builders (`from_regex`, `from_words`).
7. Cross-check conversion and equivalence against Spot via HOA in an optional integration test.

### Phase 4: quality bar
- [ ] Remove blanket `allow(unused)` and `allow(missing_docs)`; document every public item; enable `#![warn(clippy::pedantic)]` selectively.
- [ ] Add `tests/` integration tests per crate covering the public API only.
- [ ] Add property tests (`proptest`) for the laws listed under item 16, plus HOA round-trip tests using the sample files.
- [ ] Un-ignore or delete every `#[ignore]` test and every `ignore`d doctest.
- [ ] Add `criterion` benchmarks in `benches/` (product, determinize, minimize, SCC); remove the git-hash-based harness or reimplement it as a CI job that compares against `main`.
- [ ] Fuzz the HOA parser (`cargo-fuzz`).
- [ ] CI rewrite: Linux/macOS/Windows test matrix, `--no-default-features` and `--all-features` jobs, MSRV job, `cargo doc -D warnings`, `cargo deny`, `cargo semver-checks` before release, coverage upload.

### Phase 5: documentation
- [ ] Root README: one-paragraph pitch, feature table (which automata and operations exist), install snippet (`cargo add`), a 20-line quick start that actually runs (doctest), example of HOA I/O and rendering, crate map, status/MSRV/license badges, link to docs.rs and the guide.
- [ ] Per-crate README rewritten with real examples; `automata` README no longer `ignore`s code.
- [ ] Crate-level rustdoc tour in `lib.rs` (concepts: alphabet, word, transition system, automaton, acceptance, operations) with a runnable example per concept; examples on all major public functions.
- [ ] `examples/` directory: build a DFA, minimize, boolean ops, parse HOA and render, omega acceptance, learn a DFA with L*.
- [ ] mdBook guide in `docs/` (concepts, cookbook, architecture, performance notes) deployed through GitHub Pages.
- [ ] `CHANGELOG.md`, `CONTRIBUTING.md`, issue templates, and a short "comparison to Spot, `regex-automata`, `automata-lib`" section.
- [ ] Keep `CLAUDE.md` in sync with the structure after Phase 1.

### Phase 6: ecosystem
- [ ] CLI: unify the three binaries into one tool with subcommands (`convert`, `minimize`, `equiv`, `render`, `learn`), using HOA on stdin/stdout so it composes with Spot.
- [ ] Python bindings: bring into the workspace, expose DFA/NFA/omega automata and HOA I/O, publish to PyPI with `maturin` CI; or drop if there is no appetite to maintain them.
- [ ] Learning crate: stabilise the API, document each algorithm with a citation, add runnable examples and a benchmark task suite (replacing `omega-learning-tasks` data generation scripts).
- [ ] WASM build of the core library for an online playground (optional).

### Phase 7: release
- [ ] Publish `hoars`, then the core crate, then the learning crate at `0.1.0`; tag releases; set up release-plz or cargo-release.
- [ ] Announce (r/rust, the automata/verification community) with the guide and playground links.

## Suggested order of attack

1. Phase 0 in one PR (in this repo, before extraction, so the history carries the cleanups).
2. Phase 1 (extract `automata` and `hoars` into their own repos, merge core) next, since everything else touches paths and names. From here on, Phases 2 to 5 for the library happen in the new `automata` repo; Phase 6 and learning-related work stay here.
3. Phase 5 README-only slice early, so the repo has a good front page while the deeper work proceeds.
4. Phases 2 to 4 interleaved, API changes first so tests and docs are written against the final surface.
5. Phases 6 and 7 last.

## Open questions for the maintainer

- ~~Does `leonbohn/automata` exist?~~ Resolved: the old metadata pointed at `leonbohn/frieda`; an empty `leonbohn/automata` repo has been created, so Phase 1 starts fresh with filtered history.
- Single license (MIT only, or MIT OR Apache-2.0)?
- Is `automata-learning` meant to be a general-audience product or primarily research code? This decides how much API stability and documentation it needs.
- Is Python support a priority?
- Minimum Rust version to support (edition 2024 already requires 1.85+)?
