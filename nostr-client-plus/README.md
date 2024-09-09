# Nostr-plus Client

## Binaries

### Execution order

Given most of the subscriptions filter by `since` and it's set to `now`, there is an order to follow
when starting services.

`Assigner` and `Aggregator` need to start first. Their order is not important but they need to be
started before the other services.

### Configuration file

Most services require a configuration file in order to start.
We provide one in `src/bin/config.toml`; check inline comments for more information.

### Logging


Logging is controlled by `RUST_LOG` (compliant with `env_logger` syntax) and `LOG_FORMAT`.

For example
```shell
RUST_LOG=warn
LOG_FORMAT=json 
```
will set the max log level to WARN and format using a machine parsable json.  
Default is INFO and full format, that is human-readable.

### Assigner

Assigner accepts a few environment variables.  
`RELAY_URL` is optional, and it defaults to `ws://127.0.0.1:3031`.  
`ASSIGNER_PRIVATE_KEY` is mandatory.  
`MIN_HB_VERSION` is mandatory, and it is a numerical value.


Assigner needs to run with a configuration file from which it reads the whitelisted senders.
If not present or empty, it won't start.
Job messages whose senders are not listed won't be assigned and are ignored.

```shell
cargo run --release --package nostr-client-plus --bin assigner -- <config file>
```

For example, using the one provided in `nostr-client-plus/src/bin`
```shell
cargo run --release --package nostr-client-plus --bin assigner -- nostr-client-plus/src/bin/config.toml
```

### Aggregator

Aggregator accepts two environment variables: `RELAY_URL` amd `VERSION`.
`RELAY_URL` is optional and it defaults to `ws://127.0.0.1:3031`.
`VERSION` is optional and it defaults to `v0.0.1`.


```shell
cargo run --release --package nostr-client-plus --bin aggregator -- <config file>
```

For example, using the one provided in `src/bin`
```shell
cargo run --release --package nostr-client-plus --bin aggregator -- nostr-client-plus/src/bin/config.toml
```

### Publishers

Publisher needs a few environment variables:
- `MONGO_URL` is mandatory (not used by pow jobs).
- `RELAY_URL` is optional and it defaults to `ws://127.0.0.1:3031`.
- `PUBLISHER_PRIVATE_KEY` is mandatory.
- `PROMETHEUS_URL` is mandatory.
- `JOBS_THRESHOLD` is optional and defaults to 5000.
- `CLASSIFICATION_PERCENT` is optional and defaults to 100 (range is 0-100).

The publisher(_pow) is basically a one-shot program, that will publish immediately N jobs to the relay.  
When jobs are completed successfully, running it again won't pick up the same jobs, so it can be run in a cron-job.  
It's better to wait for the jobs to be completed, otherwise the same jobs will be re-submitted (we are not tracking, yet,
in-flight jobs).  
Publishers will check the metrics of a Prometheus and, if the number of acched jobs is below a configurable threshold,
it won't run.  
This makes it, again, suitable in a cron-job.

```shell
cargo run --release --package nostr-client-plus --bin publisher <num of jobs to publish> # if num is not passed, 1000 is the default
cargo run --release --package nostr-client-plus --bin publisher_pow <num of jobs to publish> # if num is not passed, 1000 is the default
```