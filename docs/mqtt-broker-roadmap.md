# MQTT 5 Broker Roadmap

## Purpose

Build a production-minded MQTT 5 broker in Rust from first principles. The project is both an implementation and an architecture exercise: protocol conformance, explicit state ownership, bounded resource use, predictable latency, and an evolutionary path toward durability and clustering.

The goal is not to compete with mature brokers immediately. Each phase should produce a usable vertical slice and evidence that its architectural claims hold.

## Architectural priorities

Ranked priorities:

1. **Correctness and protocol conformance** — explicit state machines, strict validation, interoperability, and executable specification tests.
2. **Predictable latency** — bounded work, controlled fan-out, and focus on tail latency rather than averages.
3. **Connection scalability** — efficient support for many mostly-idle, long-lived connections.
4. **Throughput** — parallelism across independent traffic without destroying ordering guarantees.
5. **Resource efficiency** — measurable per-connection and per-subscription memory budgets; minimal copying and allocation.
6. **Fault isolation** — malformed clients, slow consumers, and hot topics must have bounded blast radii.
7. **Operability** — queues, routing decisions, protocol failures, resource use, and latency must be observable.
8. **Evolvability** — QoS, persistence, authentication, alternate transports, and clustering must be addable without replacing the core.

Availability and durability are deferred features, but they influence the foundations because they require explicit ownership of sessions, subscriptions, QoS state, and retained publications.

## Foundation principles

- TCP transport, MQTT protocol semantics, and broker behavior are separate layers.
- A network connection is not an MQTT session.
- Every mutable state object has one explicit owner.
- Prefer ownership transfer and immutable sharing over shared mutation.
- Use bounded queues; there are no accidentally unbounded buffers.
- Define narrow ordering guarantees; do not imply global ordering.
- Keep the publish hot path short and measurable.
- Begin with one routing shard, but make shard count an implementation parameter rather than an architectural rewrite.
- Use Tokio primitives directly before considering an actor framework.
- Treat the MQTT specification as executable requirements through conformance tests.

## Runtime direction

The default concurrency model is:

```text
Tokio reactor
  + task per connection
  + sharded single-owner broker state
  + bounded message passing
  + immutable, reference-counted payload sharing
```

Suggested ownership:

| State | Owner |
|---|---|
| Socket, framing buffers, connection protocol state | Connection task |
| Session identity, subscriptions, QoS exchanges | Session owner |
| Topic index, routing, retained publication | Routing shard |
| Durable log and recovery state | Storage partition |
| Cluster membership and ownership map | Cluster subsystem |

## Roadmap

### Phase 1 — MQTT 5 QoS 0 vertical slice

Prove the boundaries and runtime model with TCP, CONNECT/CONNACK, SUBSCRIBE/SUBACK, QoS 0 PUBLISH, PINGREQ/PINGRESP, DISCONNECT, wildcard subscriptions, and in-memory state.

Key outcomes:

- Independently tested MQTT codec
- Explicit connection state machine
- Protocol-neutral broker commands and deliveries
- Exact, `+`, and `#` topic-filter matching
- Task-per-connection runtime and bounded mailboxes
- Single routing shard behind a shardable interface
- Interoperability with a standard MQTT 5 client
- Baseline latency, throughput, memory, and slow-consumer measurements

See `mqtt-broker-phase-1.md` for the execution plan.

### Phase 2 — QoS 1 and session semantics

Add packet identifiers, PUBACK, retransmission policy, Receive Maximum, in-flight windows, duplicate handling, and explicit clean-start/session-expiry semantics.

Architectural questions:

- Which state belongs to the connection and which survives in the session?
- Where are retry timers owned?
- What is the overload policy when a receiver's in-flight window is exhausted?
- Which delivery guarantees hold across an unclean disconnect without persistence?

Exit evidence:

- QoS 1 state-machine tests, including duplicates and reconnects
- Bounded in-flight state
- Failure-injection tests around disconnect and retransmission
- Clear documented delivery semantics for volatile sessions

### Phase 3 — Retained messages, Will messages, and complete MQTT 5 metadata

Implement retained publications, Will publication lifecycle, topic aliases, response metadata, user properties, packet-size limits, and the remaining relevant MQTT 5 properties.

Key concerns are property validation, expiry, retained-message ownership, and avoiding metadata amplification on fan-out.

### Phase 4 — Durable sessions and storage

Introduce a storage port and a first durable implementation based on a write-ahead or append-only log. Persist session state, queued messages, in-flight QoS state, retained publications, and expiry metadata as required.

Key outcomes:

- Crash-consistent recovery model
- Explicit acknowledgement/durability boundary
- Batched writes and bounded recovery time
- Storage corruption and partial-write tests
- Separation between broker semantics and storage engine

### Phase 5 — QoS 2

Implement the complete PUBLISH/PUBREC/PUBREL/PUBCOMP flow, including reconnect recovery, duplicate control packets, packet-ID lifecycle, and durable transition points.

This phase should proceed only after durable state transitions are trustworthy. QoS 2 is principally a distributed state-machine problem, not merely four packet handlers.

### Phase 6 — Security and policy

Add TLS, authentication, authorization, listener configuration, quotas, connection rate limits, topic-level policy, and protection against protocol and resource-exhaustion attacks.

Preserve a narrow authorization interface on the hot path and make policy latency observable.

### Phase 7 — Operations and extensibility

Add production configuration, graceful shutdown and draining, structured logs, metrics, traces, administrative APIs, health endpoints, overload reporting, configuration validation, and safe extension points.

Avoid placing a general plugin framework inside the routing hot path. Extensions should have explicit cost, isolation, and failure semantics.

### Phase 8 — Multi-core scale-up

Increase routing and session shard counts, establish stable partitioning, minimize cross-shard work, and benchmark skewed and wildcard-heavy workloads.

Key questions:

- Is routing partitioned by exact topic, topic prefix, tenant, or another stable key?
- How are wildcard subscriptions represented across shards?
- How are subscription mutations and publication ordering coordinated?
- How do hot topics degrade independently of unrelated traffic?

### Phase 9 — Clustering and high availability

Introduce node identity, membership, partition ownership, cross-node subscription discovery, session placement, replication, failure detection, rebalancing, and rolling upgrades.

This phase requires explicit consistency choices. A load balancer in front of independent brokers is not a stateful MQTT cluster.

## Continuous fitness functions

Track these throughout the roadmap:

- CONNECT latency: p50, p95, p99
- Publish-to-delivery latency: p50, p95, p99
- Messages and bytes per second
- Concurrent connections and connection churn
- Bytes per idle connection and per subscription
- Allocations and copied bytes per publication
- Routing lookup cost and fan-out cost
- Inbound/outbound queue depths and saturation duration
- Event-loop stalls and task scheduling delay
- Slow-consumer isolation
- Recovery time and recovered-state correctness once persistence exists

Baseline workload shapes:

1. Many idle connections
2. One publisher to many subscribers
3. Many publishers to one subscriber
4. Wildcard-heavy subscription sets
5. A deliberately slow consumer
6. A hot topic alongside unrelated traffic
7. Frequent connect/disconnect churn
8. Malformed and adversarial packet streams

## Deferred choices

Do not prematurely freeze:

- Storage engine
- Cluster consensus or replication protocol
- Cross-node session-placement scheme
- Generic plugin framework
- Stable public embedding API
- Lock-free data structures
- Actor framework

Use measurements and the semantics introduced by later phases to justify those decisions.

