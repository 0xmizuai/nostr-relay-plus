## Nostr Relayer Plus

### Env Dependencies
Rust
nodejs + pnpm - to run e2e-test in TypeScript
bun - to auto-transpile TypeScript and fast runtime
mprocs - to manage server run https://github.com/pvolok/mprocs.git

Reference to NIP-01
https://github.com/nostr-protocol/nips/blob/master/01.md
https://github.com/nostr-protocol/nips/blob/master/42.md

To run the DB: 
surreal start --log debug --user root --pass root --bind 0.0.0.0:8081 file://./surrealdb