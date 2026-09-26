# Contributing

## Rules

- Keep the dependency rule. The domain crate imports nothing from the
  workspace. The application crate imports the domain only. Run
  `cargo tree -p overcut-domain` to check.
- Put a domain invariant in a constructor. Do not check it again at a call
  site.
- Write a test for each behaviour in Given/When/Then form. Name the module
  `given_<state>` and the function `when_<action>_then_<outcome>`.
- Document every public item. Use short sentences in the active voice.
- Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` before you open a pull request.

## Adding a data source

1. Add a port trait and its record types in `crates/application/src/ports/`.
2. Add an adapter module in `crates/infrastructure/src/`. Map every
   provider quirk inside the adapter.
3. Wire the adapter in `crates/overcut/src/main.rs`.
4. Test the adapter with `wiremock`.

## Adding a use case

1. Add the input and view DTOs in `crates/application/src/dto/`.
2. Add a struct with an `execute` method in `crates/application/src/usecases/`.
3. Expose it in the CLI and the HTTP router.
