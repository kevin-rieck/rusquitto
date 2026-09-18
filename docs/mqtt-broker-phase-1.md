# MQTT 5 Broker — Phase 1 Foundation Plan

## Objective

Deliver a standards-aware MQTT 5 broker vertical slice that proves the architecture before persistence and QoS exchange state make it more expensive to change.

Phase 1 optimizes for correctness, predictable resource use, scale-up efficiency, and evolvability. It is not intended to provide durable delivery, authentication, TLS, or high availability.

## Scope

Included:

- MQTT 5 over TCP
- CONNECT / CONNACK
- SUBSCRIBE / SUBACK
- QoS 0 PUBLISH
- PINGREQ / PINGRESP
- DISCONNECT
- Exact topic filters and `+` / `#` wildcards
- In-memory connection, session, and subscription state
- Bounded internal and outbound queues
- One routing shard through a shardable abstraction
- Protocol, routing, end-to-end, and interoperability tests
- Initial metrics and performance baselines

Excluded:

- QoS 1 and QoS 2
- Persistent sessions and storage
- Retained messages
- Will messages
- TLS, authentication, and authorization
- WebSocket transport
- Shared subscriptions
- Clustering and replication
- Generic plugin system

The CONNACK capabilities must honestly advertise the limited implementation, including Maximum QoS 0, Retain Available false, wildcard subscriptions available, and shared subscriptions unavailable.

## Architectural invariants

1. **TCP != MQTT != broker.** Transport moves bytes, the protocol layer enforces MQTT semantics, and the broker core manages sessions, subscriptions, and routing.
2. **Connection != session.** Phase 1 may delete a session when its connection ends, but these remain different model concepts.
3. **Single-owner mutation.** Connection tasks own connection state; routing shards own their indexes.
4. **Bounded resource use.** Every channel, packet, payload, and pending write has an explicit limit or policy.
5. **No global ordering promise.** Preserve ingress order for publications from one connection to the same routed topic path; independent traffic may proceed concurrently.
6. **Immutable publication data.** Share payload buffers for fan-out rather than copying them per recipient.
7. **Shardability.** Phase 1 uses one shard, but protocol and connection code address it through a dispatcher that can later select among N shards.

## Component model

```text
TCP listener
    |
    +-- connection task
          | owns socket, codec buffers, protocol state
          |
          v
      BrokerCommand
          |
          v
      command dispatcher
          |
          v
      routing shard 0
      owns sessions, topic index, routing decisions
          |
          v
       Delivery
          |
          v
      bounded connection mailbox
```

The broker core must not depend on TCP streams or raw MQTT packet types. The protocol adapter translates between packets and broker commands/deliveries.

## Suggested workspace

```text
mqtt-broker/
├── Cargo.toml
├── crates/
│   ├── mqtt-codec/
│   │   └── src/
│   │       ├── packet.rs
│   │       ├── decode.rs
│   │       ├── encode.rs
│   │       ├── properties.rs
│   │       └── error.rs
│   ├── broker-core/
│   │   └── src/
│   │       ├── broker.rs
│   │       ├── command.rs
│   │       ├── delivery.rs
│   │       ├── session.rs
│   │       ├── subscription.rs
│   │       ├── topic.rs
│   │       └── routing.rs
│   └── broker-server/
│       └── src/
│           ├── connection.rs
│           ├── dispatcher.rs
│           ├── server.rs
│           └── main.rs
├── spec/
│   └── mqtt-5-conformance.md
├── tests/
│   ├── protocol/
│   ├── routing/
│   ├── end-to-end/
│   └── interoperability/
└── benches/
```

The crate boundaries correspond to different reasons to change:

| Crate | Reason to change |
|---|---|
| `mqtt-codec` | MQTT wire format and protocol rules |
| `broker-core` | Pub/sub, sessions, topic matching, and routing semantics |
| `broker-server` | Tokio networking, task lifecycle, and deployment |

## Core types

Illustrative, not frozen APIs:

```rust
enum Packet {
    Connect(Connect),
    ConnAck(ConnAck),
    Publish(Publish),
    Subscribe(Subscribe),
    SubAck(SubAck),
    PingReq,
    PingResp,
    Disconnect(Disconnect),
}

enum ConnectionState {
    AwaitingConnect,
    Connected { session_id: SessionId },
    Closing,
}

enum BrokerCommand {
    OpenSession {
        client_id: MqttClientId,
        connection: ConnectionHandle,
    },
    Subscribe {
        session_id: SessionId,
        filters: Vec<Subscription>,
    },
    Publish {
        session_id: SessionId,
        publication: Publication,
    },
    CloseConnection {
        session_id: SessionId,
    },
}

struct Publication {
    topic: TopicName,
    payload: bytes::Bytes,
    properties: PublishProperties,
}

struct Delivery {
    recipient: SessionId,
    publication: Publication,
}
```

Use typed internal identifiers such as `SessionId`, `ConnectionId`, and `ShardId`. Keep the externally supplied MQTT client identifier as a validated protocol value rather than using it as the internal primary key everywhere.

## Milestones

### 1. Specification ledger and protocol primitives

Create an executable conformance ledger mapping implemented rules to tests and MQTT specification clauses.

Implement and test:

- Fixed header and required flag validation
- Variable Byte Integer parsing and encoding
- Remaining Length framing
- UTF-8 strings and binary data
- Packet-size limits before allocation
- Property parsing, duplicate-property rules, and allowed-packet validation
- Incomplete input versus malformed input
- Symmetric encode/decode and round-trip tests

The decoder consumes buffers, not `TcpStream`. It must support fragmented frames and multiple frames in one input buffer.

Exit criteria:

- Codec unit tests require no network
- Malformed lengths cannot cause oversized allocations or panics
- Supported packets round-trip
- Unsupported packet types fail through a deliberate protocol response/close path

### 2. Topic name and filter model

Keep topic parsing, filter parsing, validation, matching, and indexing separate.

Implement:

- Topic-name validation; wildcards are forbidden
- Topic-filter validation
- Exact levels
- Single-level `+`
- Final multi-level `#`
- Empty topic levels
- Matching edge cases at separators

Build a topic tree or equivalent index with explicit exact, single-wildcard, and multi-wildcard paths. Do not begin with a full scan of all subscription strings per publication unless retained only as a correctness oracle for tests.

Exit criteria:

- Table-driven edge-case suite
- Property tests comparing the optimized matcher with a simple reference matcher
- Subscription insertion and removal tests

### 3. Broker core and ownership model

Implement protocol-neutral broker commands, session registration, subscription changes, routing, and delivery generation.

The first `RoutingShard` owns all mutable routing and session state. It runs as a single task and consumes a bounded command channel. A dispatcher maps commands to shard 0 while exposing the future N-shard seam.

Exit criteria:

- Core tests require no MQTT bytes or sockets
- One publication can fan out through shared immutable payload storage
- Disconnect removes volatile session/subscription state
- Duplicate client-ID behavior is explicitly chosen and tested
- Full queue behavior is explicit rather than accidental

### 4. Connection protocol state machine

Implement legal transitions independently of socket behavior:

```text
TCP accepted -> AwaitingConnect -> Connected -> Closing
```

Rules include:

- CONNECT must be the first MQTT packet
- A second CONNECT is a protocol error
- Only supported packets are accepted after connection
- PINGREQ produces PINGRESP
- DISCONNECT closes cleanly
- EOF and I/O failure take the unclean-close path
- Keepalive timeout is enforced or explicitly deferred with a failing/ignored conformance entry

Exit criteria:

- Transition tests cover every supported packet in every state
- Protocol errors map to the correct disconnect/close behavior
- Broker cleanup occurs on all task exit paths

### 5. Tokio transport integration

Create one task per accepted TCP connection. Each task owns its socket, codec buffers, state machine, and bounded outbound mailbox.

Use one event loop per connection to coordinate reads, outbound deliveries, keepalive deadlines, and shutdown. Avoid independent concurrent writers to the same socket.

Exit criteria:

- Fragmented and coalesced frames work over real TCP
- A slow receiver cannot cause an unbounded queue
- Cancellation and shutdown do not leak broker registration
- Per-connection task failures do not terminate the listener or routing shard

### 6. End-to-end behavior and interoperability

Verify two standard MQTT 5 clients can connect, subscribe, publish, ping, and disconnect through the broker.

Required scenario:

```text
subscriber -> CONNECT -> broker -> CONNACK
subscriber -> SUBSCRIBE sensors/+ -> broker -> SUBACK(QoS 0)
publisher  -> CONNECT -> broker -> CONNACK
publisher  -> PUBLISH sensors/temp "21" -> broker
broker     -> PUBLISH sensors/temp "21" -> subscriber
```

QoS 1 or 2 subscription requests may be accepted while granting QoS 0, consistent with the server's advertised maximum. QoS 1/2 PUBLISH packets are not silently treated as QoS 0.

Exit criteria:

- Interoperability test is repeatable in CI
- Broker capabilities match actual behavior
- Packet capture or trace can explain a failed scenario

### 7. Observability and fitness baseline

Expose at least:

- Active and total connections
- Connections by close reason
- Active sessions and subscriptions
- Publications received and deliveries attempted/completed/dropped
- Protocol errors by reason
- Command and outbound queue depth/capacity
- Publish-to-enqueue and publish-to-write latency
- Packet and payload sizes
- Task/shard processing latency

Run baseline scenarios for idle connections, fan-out, fan-in, wildcard-heavy subscriptions, a slow consumer, and malformed traffic.

Record methodology and results; absolute targets can be set after measuring the first working implementation.

## Backpressure policy

Every queue must document capacity, producer, consumer, and full-queue behavior.

Initial recommended policy:

| Queue | Full behavior |
|---|---|
| Connection to routing shard | Await briefly under a configured deadline; disconnect or reject on sustained overload |
| Routing shard to connection | For QoS 0, apply an explicit drop-or-disconnect policy; count and report it |
| Socket write buffer | Stop consuming outbound items until writable; mailbox bound contains memory |

Do not call a dropped QoS 0 publication a protocol violation or a guaranteed delivery. The chosen policy must be visible through metrics and documentation.

## Ordering contract

Phase 1 guarantees that publications processed from one connection retain their order when delivered through the same routing ownership path. It does not promise a total order across publishers or unrelated topics.

The implementation must avoid spawning detached delivery tasks that can reorder messages for one recipient. Each connection has one ordered outbound writer.

## Security baseline

Even without authentication or TLS, Phase 1 must defend its parser and runtime boundaries:

- Enforce maximum packet size before allocation
- Validate all lengths and integer overflows
- Bound UTF-8, property, subscription, and topic counts
- Use timeouts for incomplete CONNECT and stalled writes
- Reject invalid fixed-header flags
- Avoid panics on untrusted input
- Fuzz codec and topic-filter parsers
- Never log arbitrary payloads by default

## Phase 1 definition of done

| Area | Done when |
|---|---|
| Framing | Fragmented and coalesced TCP packets decode correctly |
| CONNECT | A standard MQTT 5 client connects; illegal sequencing is rejected |
| CONNACK | Advertised capabilities exactly match implementation |
| SUBSCRIBE | Exact, `+`, and `#` filters work, including edge cases |
| SUBACK | QoS 0 is granted within advertised limits |
| PUBLISH | QoS 0 fan-out routes correctly without payload copies per subscriber |
| Ordering | Documented per-connection/path ordering is maintained |
| Backpressure | All queues are bounded and overload behavior is tested |
| Ping | PINGREQ/PINGRESP works and keepalive policy is explicit |
| Disconnect | Clean and unclean exits release volatile state |
| Errors | Malformed and protocol-error packets terminate correctly without panic |
| Isolation | A slow or malformed client does not stall unrelated connections indefinitely |
| Observability | Core state, errors, queue pressure, and latency are measurable |
| Interoperability | Standard MQTT 5 CLI/client publish-subscribe scenario passes |
| Architecture | Codec and broker-core tests require no network; shard count is not embedded in protocol code |

## Decisions to record as ADRs

Before or during implementation, capture:

1. Rust and Tokio as implementation/runtime choices
2. Task-per-connection plus sharded single-owner state
3. Broker-core isolation from MQTT packets and TCP
4. Topic-index representation
5. Queue capacities and Phase 1 overload policy
6. Duplicate client-ID behavior
7. Ordering contract
8. Error taxonomy and connection-close behavior
9. Metrics surface and benchmark methodology

## First implementation slice

Start narrower than the whole phase:

1. Workspace and crate boundaries
2. Variable Byte Integer and frame boundary decoder
3. CONNECT decode plus connection transition
4. CONNACK encode
5. One TCP connection reaching the Connected state

Then add topic types and the broker path. This establishes a real vertical slice early without coupling the codec directly to the broker.

