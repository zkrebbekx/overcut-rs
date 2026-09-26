# Architecture

overcut-rs is a Rust port of [overcut](https://github.com/zkrebbekx/overcut),
an F1 Fantasy analysis toolkit. The port follows domain-driven design and
clean architecture. This document explains the layers, the dependency
rule, and where each concept from the Go original lives.

## The dependency rule

The workspace has four crates. Each crate is one layer. A crate depends
only on the crates inside it.

```
+---------------------------------------------------------------+
| overcut (interface)        CLI (clap), HTTP API (axum), wiring |
|  +----------------------------------------------------------+ |
|  | overcut-infrastructure   HTTP gateways, JSON file store   | |
|  |  +-----------------------------------------------------+ | |
|  |  | overcut-application   use cases, ports, DTOs        | | |
|  |  |  +-----------------------------------------------+  | | |
|  |  |  | overcut-domain      rules, season, projection, |  | | |
|  |  |  |                     optimizer, pricing, backtest| | | |
|  |  |  +-----------------------------------------------+  | | |
|  |  +-----------------------------------------------------+ | |
|  +----------------------------------------------------------+ |
+---------------------------------------------------------------+
```

Cargo enforces the rule. `overcut-domain` has no workspace dependency.
`overcut-application` depends on the domain only. `overcut-infrastructure`
depends on the application and the domain. The binary crate depends on all
three and does the wiring.

The domain has no I/O and no `serde`. It cannot read a file, call an API,
or print. Every side effect goes through a port.

## The domain

The domain crate holds the ubiquitous language of the game.

| Module | Concept | Kind |
| --- | --- | --- |
| `shared` | `Tla`, `AssetId`, `TeamId`, `RoundNumber`, `Gameday`, `AssetKind` | Value objects. Each constructor validates. |
| `rules` | `ScoringRules`, `DriverWeekend`, `Breakdown` | Value object and pure functions. `ScoringRules::default()` is the 2026 table. |
| `season` | `Season` (root), `Round`, `Asset`, `GamedaySnapshot` | Aggregate. `Season::new` checks the invariants. |
| `season::policy` | Sync due, provisional points | Domain policy with named constants. |
| `season::SeasonRepository` | Load and save the aggregate | Port. The domain owns it because it persists an aggregate. |
| `projection` | `PaceModel`, `WeekendConditions`, `SimulationResult` | Domain service. Fit, then Monte Carlo. |
| `optimizer` | `Candidate`, `OptimizerOptions`, `Lineup`, `best_lineups` | Domain service. Exact enumeration. |
| `pricing` | `PriceModel`, `examples`, `backtest` | Domain service. |
| `backtest` | `run`, `BacktestReport` | Domain service. |

Every invariant lives in one place. A `Tla` is always upper-case. A
`RoundNumber` is never zero. A `Season` never holds two rounds with one
number. A caller cannot build an invalid value, so the code that uses the
value does not check it again.

## The application

A use case is one operation a user can ask for. Each use case is a struct
with an `execute` method. It takes a plain input, calls the domain, and
returns a view.

| Use case | Input | View |
| --- | --- | --- |
| `SeasonOverview` | the clock | `SeasonView` |
| `ProjectRound` | `ProjectInput` | `ProjectionView` |
| `OptimizeTeam` | `OptimizeInput` | `OptimizeView` |
| `ReviewRound` | `ReviewInput` | `ReviewView` |
| `PredictPrices` | none | `PricesView` |
| `RunBacktest` | sims | `BacktestView` |
| `Hindsight` | round, top | `HindsightView` |
| `SyncSeason` | year, when_due | `SyncView` and the new `Season` |

`AnalysisContext` holds the season, the rules, and the caches of expensive
results. It is safe to share across threads. A simulation runs outside the
lock, so a long computation never blocks a reader. A sync replaces the
season and clears the caches.

The views carry `serde` derives. They are the API contract. The JSON field
names match the Go service, so the original React client works unchanged.

### Ports

A port is a trait the application owns. An adapter implements it.

| Port | Purpose | Adapter |
| --- | --- | --- |
| `RaceDataGateway` | calendar and classifications | `JolpicaClient` |
| `FantasyFeedGateway` | prices, ownership, official points | `FantasyFeedClient` |
| `StartingGridGateway` | the official grid with penalties | `OfficialSiteClient` |
| `Clock` | the current time | `SystemClock` |
| `SeasonRepository` (domain) | the season file | `JsonSeasonRepository` |

The record types a port exchanges are source-agnostic. A quirk of one
provider stays inside its adapter. Two examples:

- The classification source encodes every number as a string. The adapter
  parses them.
- The fantasy feed labels the sprint race "Sprint Qualifying". The adapter
  adds both labels into the sprint points.

## The infrastructure

Each adapter is one module with its own error type. HTTP adapters use
`reqwest` with a 30-second timeout and map every failure to a
`GatewayError`. The file store keeps the exact JSON format of the Go
version, so the committed data file loads without conversion. The store
writes to a temporary file and renames it, so a crash never leaves a
half-written season.

The domain types have no `serde`. The file store defines private DTOs that
mirror the file format and maps them to and from the domain. This keeps
the file format and the domain model free to change on their own.

## The interface

The binary crate has two entry points that call the same use cases:

- `cli`: one `clap` subcommand per use case. Output goes through
  `tabwriter` for aligned tables.
- `http`: an `axum` router with the same routes as the Go server. CPU-bound
  use cases run in `spawn_blocking`. Errors map to JSON `{"error": ...}`
  with 400 for a client error, 502 for a gateway failure, and 500
  otherwise.

## From Go to Rust

| Go | Rust | Note |
| --- | --- | --- |
| `rules.Config` struct with JSON tags | `ScoringRules` (domain) + `RulesDto` (application) | The domain type has no serde. The DTO overlays a partial file onto the defaults. |
| `dataset.Data` with methods | `Season` aggregate with private fields | `Season::new` validates. Callers use accessors. |
| `map[string]int` keyed by TLA | `BTreeMap<Tla, u32>` | Ordered, so output is deterministic. |
| `string` kind `"driver"` | `enum AssetKind` | The compiler checks every match. |
| `model.GridInfluence` global var | `SimulationSettings.grid_influence` | No global state. |
| `math/rand/v2` PCG | `rand_pcg::Pcg64Dxsm` | Same family. The streams differ, so results match in distribution, not bit for bit. |
| `(value, bool)` returns | `Option<T>` | |
| `error` returns | `Result<T, DomainError>` with `thiserror` | Each error is a variant a caller can match. |
| `sync.RWMutex` engine | `AnalysisContext` with `RwLock` | Compute runs outside the lock. |
| `engine.Engine` methods | One struct per use case | Each use case is testable alone. |
| `interface` for HTTP client | `#[async_trait]` port | Gateways are `Arc<dyn Trait>`. |
| goconvey Given/When/Then | `mod given_x { fn when_y_then_z() }` | Same structure with plain `#[test]`. |

## Verification

The tests at every layer are ported from the Go suite and extended. The
`parity` integration tests load the committed 2026 season and check the
Rust engine against numbers the Go binary produced on the same data:

- The rules engine reproduces the official qualifying points on every
  driver-round.
- The hindsight optimum for rounds 12 and 14 is the same team with the same
  score and cost.
- The price predictor reports the same walk-forward error and direction
  hit rate.
- The projection means for the next round fall inside a tolerance of the
  Go means. The Monte Carlo streams differ, so the tolerance is
  statistical.
