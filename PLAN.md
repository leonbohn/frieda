# PLAN: making FRIEDA a top-notch general-purpose automata crate

Goal: a published, documented, well-tested Rust crate for finite- and omega-word automata (DFA, NFA, Moore/Mealy, DBA/NBA, DPA/NPA, Rabin, Muller, ...) with HOA interoperability, with learning algorithms as an optional layer.

Work is split into phases. Each phase leaves the repo green (`./check` passes) and is independently mergeable. Do them in order; items inside a phase can be tackled in any order.

## Current state (assessment; written before the repositories were split, so it describes the former monorepo)

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

### Phase 1: split into focused repositories (status: done, except publishing)
The libraries were extracted with history preserved:

| Repo | State |
|---|---|
| `leonbohn/automata` | `automata` + `automata-core` merged (core is `src/core/` for now), pushed; its `PLAN.md` tracks the follow-ups (flatten `core`, API redesign, publishing). |
| `leonbohn/hoars` | pushed and published as `hoars 0.3.0` on crates.io (breaking: `biodivine-lib-bdd` 0.6). |
| `leonbohn/automata-learning` | pushed (79 commits: original history, import of the monorepo state, replayed monorepo commits, standalone commit). |

This repo now only contains `bin/*`, `python/` and `assets/`, using git dependencies on `automata` and `automata-learning`.

Remaining here:
- [ ] Once `automata` (and `automata-learning`) are published, replace the git dependencies with versions.
- [ ] Merge the three binaries into one CLI with subcommands (Phase 6).
- [ ] Decide the fate of `python/` (Phase 6); it still builds against the git `automata`, which is untested.
- [ ] Rewrite the README (Phase 5 of `automata`'s plan covers the library docs; this README should become a short index plus tool docs).

The original plan items for the remaining phases (API redesign, algorithms, quality, docs, ecosystem, release) now apply to the individual repositories; see their `PLAN.md` files.

### Phase 6: ecosystem
- [ ] CLI: unify the three binaries into one tool with subcommands (`convert`, `minimize`, `equiv`, `render`, `learn`), using HOA on stdin/stdout so it composes with Spot.
- [ ] Python bindings: bring into the workspace, expose DFA/NFA/omega automata and HOA I/O, publish to PyPI with `maturin` CI; or drop if there is no appetite to maintain them.
- [ ] Learning crate: stabilise the API, document each algorithm with a citation, add runnable examples and a benchmark task suite (replacing `omega-learning-tasks` data generation scripts).
- [ ] WASM build of the core library for an online playground (optional).

## Next steps

1. Library work continues in the `automata`, `hoars` and `automata-learning` repositories (see their `PLAN.md` files); publish `automata` after a bug-fixing and code review pass.
2. Here: switch to released dependency versions, then the CLI merge and the README rewrite.

## Open questions for the maintainer

- ~~Single license?~~ Resolved: MIT.
- Is `automata-learning` meant to be a general-audience product or primarily research code? This decides how much API stability and documentation it needs.
- Is Python support a priority?
- Minimum Rust version to support (edition 2024 already requires 1.85+)?
