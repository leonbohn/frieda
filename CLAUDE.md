# CLAUDE.md

Guidance for working in this repository. See `PLAN.md` for the roadmap and known problems.

## What this is

FRIEDA is a Rust workspace for working with transition systems and automata over finite and infinite (omega) words, plus automata learning algorithms. Roughly 27k lines of Rust, edition 2024, resolver 3.

## Commands

```shell
cargo build --workspace
cargo test --workspace            # ~145 tests pass, ~14 ignored
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo doc --workspace --no-deps
./scripts/check                   # fmt + clippy (all targets/features) + doc + test
cargo run -p oai -- todpa < in.hoa
```

`default-members` is set in the root `Cargo.toml`; `python/` is excluded from the workspace and has its own manifest. Shared package metadata (edition, license, repository, authors) lives in `[workspace.package]`; `Cargo.lock` is committed.

## Planned restructuring

This layout is transitional. The plan (see `PLAN.md`, Phase 1) is to move `automata` (with `automata-core` merged in) into its own single-crate repository `leonbohn/automata`, move `hoars` into its own repository, and keep this repo (`frieda`) as the research umbrella for `automata-learning`, the CLI tools, Python bindings and benchmarks, depending on the published `automata` crate. Until that happens, the description below is accurate. Do not add new code to `automata-core` that assumes it remains a separate crate: its modules are going to become top-level modules of `automata`.

## Workspace layout

| Path | Crate | Role |
|---|---|---|
| `crates/hoars` | `hoars` (0.2.1) | Independent parser/printer for the HOA (Hanoi Omega Automata) format. Uses `chumsky` and `ariadne` for parsing and diagnostics, `biodivine-lib-bdd` for label expressions. |
| `crates/automata-core` | `automata-core` (0.1.0) | Foundations: alphabets, words, shared math types, DAGs, `Color`, `Lattice`, `Show`. |
| `crates/automata` | `automata` (0.0.4) | Main library: transition systems, automata, acceptance conditions, SCCs, products, minimization, congruences, HOA bridge, rendering, random generation. |
| `crates/automata-learning` | `automata-learning` | Passive (RPNI-style `sprout`, DBAInf, DPAInf, FWPM) and active (L*, Mealy/Moore) learning. |
| `bin/oai` | | CLI: reads HOA from stdin, converts to deterministic parity (`todpa`). |
| `bin/incong` | | Small tool enumerating periodic omega words up to rotation. |
| `bin/omega-learning-tasks` | | Generates learning tasks and runs sprout over them. |
| `python/` | `automata_python` | PyO3/maturin bindings (only a `DFA` wrapper so far; not in the workspace). |

Dependency direction: `hoars` and `automata-core` are leaves, `automata` depends on `automata-core` (and optionally `hoars`), `automata-learning` and the binaries depend on `automata`.

## Architecture

### automata-core

- `alphabet`: `Symbol` (Copy, Ord, Hash), `Expression` (a label on an edge, `Matcher` decides which symbols it matches), and `Alphabet` (associated `Symbol`/`Expression`/`Universe`). Implementations: `CharAlphabet`, `Fixed<S, N>`, `Directional`/`InvertibleChar` (simple alphabets), and `PropAlphabet`/`PropExpression`/`PropSymbol` (propositional, BDD-backed, used for HOA).
- `word`: `Word` with `FiniteWord` and `OmegaWord` sub-traits. Concrete types: `PeriodicOmegaWord`, `ReducedOmegaWord` (prefix + cycle), `NormalizedOmegaWord`, plus adapters (`Concat`, `Repeat`, `Skip`, `Infix`, `Rotate`).
- `math`: type aliases for sets/maps/bijections, `dag`: DAG representation (used for SCC condensation).
- `Color` is blanket-implemented for any `Clone + Eq + Hash + Debug`; `Void` is the "no color" type; `Lattice` is an ordered finite set of colors.

### automata

Central abstraction is the `TransitionSystem` trait (`src/ts.rs`). It is an interface over:
- an `Alphabet`, a `StateIndex`, `StateColor`, and `EdgeColor`,
- GATs for edge references and iterators (`EdgesFromIter`, `StateIndices`).

Edges are labeled with alphabet *expressions*; a *transition* is the concrete (state, symbol) step. Capabilities are layered through extra traits rather than one large trait:
- `Deterministic` (successor lookup by symbol), `Pointed` (has an initial state),
- `Sproutable` (can add states and edges), `Shrinkable` (can remove them),
- `PredecessorIterable`, `CollectTs`/`IntoTs` (conversion between representations).

Key modules under `src/ts/`:
- `impls/`: `LinkedListTransitionSystem` (default, aliased as `TS`/`DTS`/`NTS` in `lib.rs`), edge-list based variants, and a `petgraph`-backed one (feature `petgraph`).
- `operations/`: lazy adapters (`map`, `product`, `quotient`, `restricted`, `reverse`, `subset`, `with_state_color`).
- `run.rs` + `Observer`s: runs of words through a TS, collecting states, colors, or limits; `path.rs` has `Path` and `Lasso`.
- `connected_components/`: Tarjan SCC and `SccDecomposition`. `reachable.rs`: reachability and length-lexicographic minimal representatives.
- `builder.rs`: `TSBuilder`, the ergonomic entry point for constructing TSs and automata.

`Automaton<A, Z, Q, C, D, const OMEGA, const DET>` (`src/automaton.rs`) pairs a TS `D` with an initial state and an acceptance condition `Z`. `OMEGA` selects finite vs. infinite-word semantics, `DET` selects determinism. Type aliases (`DFA`, `DBA`, `DPA`, `MooreMachine`, `MealyMachine`, ...) are defined in `src/automaton/`. Acceptance conditions live in `automaton/omega/` (`buchi`, `parity` with four min/max even/odd variants, `rabin`, `muller`) and implement the `Semantics` trait. Moore/Mealy are expressed as semantics over colors.

Other modules: `congruence/` (right congruences, Cayley graphs, transition profiles), `minimization/partition_refinement.rs`, `families/` (FDFA, FWPM conversions), `hoa/` (feature `hoa`, input and output via `hoars`), `dot.rs` and `render` (Graphviz output and SVG rendering via `layout-rs`/`resvg`), `random/` (feature `random`).

Features: `default = ["hoa", "random", "render"]`, plus `graphviz`, `petgraph`, `implementations`.

### automata-learning

- `passive/`: `sample/` (positive/negative word samples, `characterize`), `sprout/` (consistency checking and the sprout algorithm), `dpainf`, `precise`, `fwpm`.
- `active/`: `observationtable`, `lstar`, `oracle`, `hypothesis`, `mealy`, `moore`.
- `priority_mapping.rs`: annotated congruences and weak priority mappings used by the passive omega learners.

## Conventions

- `automata` has `#![deny(missing_docs)]` and `deny(rustdoc::broken_intra_doc_links)`; `automata-learning` currently opts out with blanket `allow(missing_docs)`/`allow(unused)`.
- The crate README is `include_str!`ed into the `automata` crate docs, so README code blocks are doctests (several are `ignore`d).
- CI (`.github/workflows/rust.yml`) runs fmt, clippy `-D warnings`, `cargo doc`, build, and test.
- Use `tracing` for logging, not `println!`, in library code.
- Mind the coherence between the type parameters: most generic code is written against `TransitionSystem<...>` bounds, so adding a capability usually means a new trait or a blanket impl, not a new method on `TransitionSystem`.

## Known sharp edges

See `PLAN.md` for the full list. The ones most likely to bite when editing:
- `Automaton` is parameterized by seven generic arguments; type errors can be long.
- `python/` is not built by CI and has drifted from the workspace.
