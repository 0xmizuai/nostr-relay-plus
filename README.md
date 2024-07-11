## Nostr Relayer Plus

### Env Dependencies
* **Rust**
* **nodejs + pnpm** - to run e2e-test in TypeScript
* **bun** - to auto-transpile TypeScript and fast runtime
* **mprocs** - to manage server run https://github.com/pvolok/mprocs.git

Reference to NIP-01  
https://github.com/nostr-protocol/nips/blob/master/01.md  
https://github.com/nostr-protocol/nips/blob/master/42.md

To run the DB locally:

`surreal start --log debug --user root --pass root --bind 0.0.0.0:8081 file://./surrealdb`

### Run the relay server

#### Locally
Starts the server with local database (see above)

`cargo run --release --package nostr-relay`

#### Remotely

Starts the server with remote database.

`cargo run --release --package nostr-relay -- --remote`

The following environment variable must be set
```shell
SURREAL_URL
SURREAL_USER
SURREAL_PASS
```
**SURREAL_URL** must be a wss address.