# CLAUDE.md

Guidance for working in this repository. See `PLAN.md` for the roadmap.

## What this is

`frieda` is the umbrella repository for tools and experiments built on top of the automata libraries. The libraries themselves used to live here and have been extracted, with history, into their own repositories:

| Repository | Local path | Contents |
|---|---|---|
| `leonbohn/automata` | `/home/lonne/Rust/automata` | transition systems, automata, acceptance conditions, constructions, HOA bridge, rendering (the former `automata` + `automata-core`) |
| `leonbohn/hoars` | `/home/lonne/Rust/hoars` | HOA parser/printer; published on crates.io (0.3.0) |
| `leonbohn/automata-learning` | `/sam/Rust/automata-learning` | passive and active learning algorithms |

Each of those has its own `CLAUDE.md` and `PLAN.md`. Do not re-add library code here.

## Layout

| Path | Crate | Role |
|---|---|---|
| `bin/oai` | `oai` | CLI: reads HOA from stdin, converts to a deterministic parity automaton (`todpa`). |
| `bin/incong` | `incong` | Enumerates periodic omega words up to rotation. Long-running by design. |
| `bin/omega-learning-tasks` | `omega-learning-tasks` | Generates learning tasks and runs sprout over them (see its README). |
| `python/` | `automata_python` | PyO3/maturin bindings; only a `DFA` wrapper; excluded from the workspace and not built by CI. |
| `assets/` | | Logo for the README. |

The workspace (`Cargo.toml`) has members `bin/*`. `automata` and `automata-learning` are workspace dependencies on the `main` branch of their GitHub repositories (git dependencies, until they are published). A breaking change on their `main` can break the binaries; run `cargo update -p automata` deliberately and pin a `rev` if needed. `Cargo.lock` is committed.

## Commands

```shell
cargo build --workspace
cargo test --workspace            # the binaries have no tests
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/check                   # fmt + clippy + doc + test, same as CI
cargo run -p oai -- todpa < file.hoa
```

When developing a library change together with a binary, use a local override instead of editing the manifests, e.g. in `.cargo/config.toml`:

```toml
[patch."https://github.com/leonbohn/automata"]
automata = { path = "/home/lonne/Rust/automata" }
```

## Conventions

- Use `tracing` for logging; binaries set up `tracing-subscriber`.
- Shared package metadata (edition, license, repository, authors) is in `[workspace.package]`; binaries are `publish = false`.
- Ask before committing or pushing.
