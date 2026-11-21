## Style
* economy of complexity is main architecture goal
* Prefer `expect` over `unwrap` to document invariants
* Use Rust edition 2024. `mod.rs` is forbidden

## Embedded
* static dispatch is preferable

## Tests
* Only feature tests are allowed
* To add a feature, add requirements, write user scenarios for those requirements, then add feature tests for those scenarios
* Every user scenario has exactly one feature test, and every feature test corresponds to exactly one user scenario.
* Name every feature test after its user scenario in snake case, omitting articles. `A user creates a chain` becomes `user_creates_chain`.
