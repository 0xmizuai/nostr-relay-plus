# Nostr-plus Client

## Binaries

### Execution order

Given most of the subscriptions filter by `since` and it's set to `now`, there is an order to follow
when starting services.

`Assigner` and `Aggregator` need to start first. Their order is not important but they need to be
started before the other services.

### Assigner

Assigner accepts one environment variable: `RELAY_URL`.
`RELAY_URL` is optional and it defaults to `ws://127.0.0.1:3031`.

Assigner needs to run with a configuration file from which it reads the whitelisted senders.
If not present or empty, it won't start.
Job messages whose senders are not listed won't be assigned and are ignored.

```shell
cargo run --release --package nostr-client-plus --bin assigner -- <config file>
```

For example, using the one provided in `src/bin`
```shell
cargo run --release --package nostr-client-plus --bin assigner -- nostr-client-plus/src/bin/config.toml
```

### Publisher

Publisher needs reads 2 environment variables: `MONGO_URL` and `RELAY_URL`.  
`MONGO_URL` is mandatory.  
`RELAY_URL` is optional and it defaults to `ws://127.0.0.1:3031`

The publisher is basically a one-shot program, that will publish immediately N jobs to the relay.  
When jobs are completed successfully, running it again won't pick up the same jobs, so it can be run cyclically.  
It's better to wait for the jobs to be completed, otherwise the same jobs will be re-submitted (we are not tracking, yet,
in-flight jobs).

```shell
cargo run --release --package nostr-client-plus --bin assigner <num of jobs to publish> # if num is not passed, 1000 is the default
```