# Demo

## Job assignment protocol

### Basic version
- The Publisher posts an Event (JobPost, kind == 6000) with
  `content` containing the job details.
- Workers are supposed to subscribe to events with kind == 6000
- If workers are willing to accept the job, they post a JobBooking
  event (kind == 6001), with tag `#e` set to the id of the JobPost they are willing
  to take.
- Publishers subscribe to events with kind == 6001 and tag `#e` set
  appropriately
- The publisher implements its own logic to decide the assignment.
- When the winner is decided, a JobAssigned event (kind == 6002) is sent,
- with tag `#e` set the original JobPost id and `#p` set to the address
  of the winner.
- Workers subscribe to events of kind == 6002 with tags set appropriately,
  to know if they have got the job.

### Later phase
- Does the winner need to ACK, so the Publisher is sure someone is working on it?
- How does it work when the publisher does not see the job done within a certain amount of time?

## Demo binaries

### Publisher

```shell
cargo run --package nostr-client-plus --bin publishe
```

### Worker

```shell
cargo run --package nostr-client-plus --bin worker <salt id>
```

`salt id` is used to give every worker a different identity. It must be a u8 number.

### Relay

```shell
surreal start --log debug --user root --pass root --bind 0.0.0.0:8081 file://./surrealdb &>/dev/null &
cargo run --package nostr-relay &
```
### Issues

The code is very basic, but it should be able to show how workers compete
and who the publisher assigns the tak to.

However, the event returned for the subscription, do not contain any tags (although the
events contain them). For this reason has not been possible to run multiple workers
concurrently and successfully: they all though they won, because the `#p` tag was ignored
when subscribing and the returned event does not contain any tags, so the workers cannot
understand who is the winner.