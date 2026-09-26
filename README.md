# overcut-rs

A Rust port of [overcut](https://github.com/zkrebbekx/overcut), an F1
Fantasy analysis toolkit. One binary, a CLI, and a local JSON API. No
account. No subscription.

overcut answers one question on a race weekend: **given my team, what
should I do this round, and how confident should I be?**

This repository exists for two reasons:

1. It is a complete, working toolkit. It reads the same data file as the Go
   version, serves the same API, and produces the same answers.
2. It is a reference for domain-driven design and clean architecture in
   Rust. Every layer is its own crate. The compiler enforces the dependency
   rule. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## What it does

- **Decide.** Enter your team, transfers, cash, and chip. The optimizer
  enumerates every legal team, about 1.4 million driver and constructor
  combinations, and returns the provable optimum for the projections with
  the transfer penalty priced in.
- **Project.** A Monte Carlo model simulates the weekend many thousand
  times and gives every asset a full points distribution: mean, P10, P50,
  and P90.
- **Chips.** Every chip is valued against the recommended team each round:
  No Negative, x3 Boost, Autopilot, Wildcard, Limitless, and Final Fix.
- **Know the weekend.** The model uses everything that is official: the
  qualifying classification, the starting grid with every penalty applied,
  and the sprint result. Anything not yet known can be entered by hand.
- **Review.** For any finished round: what the model expected on Sunday
  morning against what happened, ranked by surprise.
- **Prices.** A predictor for the next price change, fitted on the season's
  real price moves, with its own measured error.
- **Trust.** A walk-forward backtest replays the season and scores the
  model against two naive strategies.
- **Hindsight.** The best possible team for any finished round.

## Install

```bash
cargo install --git https://github.com/zkrebbekx/overcut-rs overcut
overcut sync          # download the season from public sources
overcut serve         # JSON API on http://127.0.0.1:8080
```

Or build from a clone:

```bash
cargo build --release
./target/release/overcut --help
```

The repository ships the 2026 season in `fixtures/season2026.json`. Copy
it to `~/.overcut/season2026.json` to work offline without a sync.

## CLI

```bash
overcut project                                    # next round, all assets
overcut optimize --team ANT,HUL,BOR,COL,LIN,Mercedes,McLaren --free 3 --budget 121.8
overcut optimize --back ANT,ALB --fp3 RUS,HAM,VER  # with known weekend state
overcut optimize --chip 3x                         # chips: wildcard | limitless | 3x | nonegative
overcut prices
overcut backtest
overcut hindsight --round 12
overcut review --round 12 --team GAS,COL,HUL,ANT,LIN,Mercedes,Ferrari
overcut sync --when-due                            # only when a session just ended
overcut serve --ui path/to/web/dist                # serve the React UI from the Go repo
```

Every command accepts `--data PATH`, `--season YEAR`, and `--rules PATH`.
`sync` writes `~/.overcut/season<year>.json`. Every other command works
offline from that file and is deterministic for a given seed.

## API

| Method | Route | Use case |
| --- | --- | --- |
| GET | `/api/season` | the calendar and every asset |
| GET | `/api/rules` | the active scoring rules |
| GET | `/api/project?round=&sims=&seed=&quali=&grid=&back=&fp3=` | project a round |
| POST | `/api/optimize` | find the best team |
| GET | `/api/prices` | predict price changes |
| GET | `/api/backtest?sims=` | measure the model |
| GET | `/api/hindsight?round=&top=` | best team for a past round |
| POST | `/api/review` | projection against actual |
| POST | `/api/sync` | download the season |
| GET | `/healthz` | liveness |

The routes and JSON shapes match the Go service. The React UI from the Go
repository works against this server without a change.

## Read the code

The crates are small. Read them in this order to see how Rust expresses
the design.

| Start here | What it shows |
| --- | --- |
| `crates/domain/src/shared/ids.rs` | Newtypes as value objects. A `Tla` cannot be lower-case. A `RoundNumber` cannot be zero. |
| `crates/domain/src/rules/` | A value object with pure methods. `Default` for the 2026 tables. Struct update syntax in tests. |
| `crates/domain/src/season/aggregate.rs` | An aggregate root with private fields, a validating constructor, and iterator-returning accessors. |
| `crates/domain/src/season/repository.rs` | A port as a trait. A boxed error so adapters stay independent. |
| `crates/domain/src/projection/simulate.rs` | Ownership in a hot loop. Closures that borrow. A seeded RNG instead of a global. |
| `crates/domain/src/optimizer/` | A hand-written `Iterator`. Slices and `partition_point`. |
| `crates/application/src/usecases/context.rs` | `Arc` and `RwLock` for shared state. Compute outside the lock. |
| `crates/application/src/usecases/optimize_team.rs` | A use case that composes domain services and returns a view. |
| `crates/application/src/ports/` | `#[async_trait]` ports with source-agnostic records. |
| `crates/infrastructure/src/file_store.rs` | Private serde DTOs mapped to a serde-free domain. Atomic file write. |
| `crates/infrastructure/src/jolpica.rs` | An async HTTP adapter with pagination and lenient parsing. |
| `crates/overcut/src/http/` | An `axum` router, `spawn_blocking`, and error mapping to status codes. |
| `crates/overcut/src/cli/` | `clap` derive with subcommands. |

Tests follow Given/When/Then. A test module is the "given", and each test
function is one "when ... then". Run them with `cargo test --workspace`.

## Data

Three public sources, no authentication:

- Schedule with session times, and the qualifying, sprint, and race
  classifications, from the Jolpica F1 API.
- The official starting grid, with penalties, from the Formula 1 results
  pages.
- Prices, ownership, and official fantasy points from the F1 Fantasy game's
  public feed.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --workspace --no-deps --open
```

CI runs the same four commands on every push and pull request.

## License

MIT
