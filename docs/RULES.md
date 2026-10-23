# Rules

Project rules from the default workflow apply: secure code, single responsibility, no duplication, design fidelity, no emoji, no AI patterns, no em or en dashes, no AI attribution in git, no commits unless asked.

## Rust
- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` must pass.
- No `unwrap` or `expect` outside tests and startup. Library errors use `thiserror`.
- SQL is parameterized only. Handlers validate input with typed structs.
- Passwords hashed with argon2. Secrets from environment only.
- Engine crate has no IO, no clock, no randomness except through a passed seed.

## Web
- TypeScript strict. No `any`.
- Components small and single purpose. Server state in TanStack Query, local UI state in React.
- Tokens from `docs/DESIGN.md` only. Keyboard and screen reader support required.
- Respect `prefers-reduced-motion`.

## Testing
- Engine: unit tests for every format, odd team counts, edit propagation.
- Server: integration tests against in memory SQLite.
