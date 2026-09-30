# apro-client

Client SDK for the **APRO Works** orchestration store.

Your app does not need to know anything about the platform's database, its HTTP
routes or its schema. It pushes artifacts by name, pulls them back by name, and
declares what it depends on. APRO Works handles the rest.

```toml
[dependencies]
apro-client = { git = "https://github.com/APRO-pk/apro-client", tag = "v0.1.0" }
```

That is the only dependency an app needs. The store itself (SQLite, the HTTP
service, the hub) is not linked in — `apro-client` carries only the wire types
and one small blocking HTTP transport.

Payloads that two apps exchange are described in a companion crate:

```toml
apro-contracts = { git = "https://github.com/APRO-pk/apro-client", tag = "v0.2.0" }
```

---

## Payload contracts

The hub never parses a payload, so it cannot catch a unit error. `apro-contracts`
is where the apps that *do* exchange data agree on what the bytes mean — and, more
importantly, where the conversions live.

| Crate | Purpose |
| --- | --- |
| `apro-types` | Platform vocabulary: artifact ids, revisions, edges, modes |
| `apro-contracts` | Payload shapes two apps agree on, plus the unit/datum conversions |
| `apro-client` | The transport and the launch handshake |

`apro-contracts` depends on nothing but `serde`. Each app maps its own types in
and out, so it never needs a CAD kernel or a flight-dynamics model to take part.

### Units are the whole problem

aproCAD works in millimetres. HexaDOF is SI. Mass is already kilograms on both
sides, so a spot-check of the mass looks fine while the inertia is wrong by a
factor of **one million** — which produces flight dynamics that are merely
sluggish, not obviously broken.

| quantity | factor (mm → m) |
| --- | --- |
| mass | 1 |
| centre of gravity | 1000 |
| inertia | **1e6** |
| volume | 1e9 |

So the payload declares its units, and `validate()` refuses anything that is not
`SI` rather than guessing.

### The datum is declared, never inferred

A CAD origin is wherever the author put it; a body datum is usually the nose tip,
the base, or the CG. Nothing in either model can tell you which. The payload
therefore carries `datum_offset_m` — where the CAD origin sits in the body-datum
frame, so `p_body = p_cad + datum_offset_m`. Use
`center_of_gravity_in_datum()` rather than doing the shift yourself; it is
implemented and tested once, including the part that is easy to get wrong:
inertia is *unchanged*, because it is about the CG, not the origin.

---

## The mental model

The platform does **not** define a universal object schema. Describing every
artifact every app might ever produce is a losing game, so the store is
deliberately blind instead:

| Concept | Meaning |
| --- | --- |
| **Artifact** | A named slot, identified by `type_id × instance × encoding` |
| **Revision** | One immutable, content-addressed version of an artifact |
| **Edge** | "My instance *consumes* that instance, at this revision" |
| **Subscription** | "My app consumes that *type*" — durable, materialised into edges |

The hub stores bytes and remembers lineage. It never parses a payload. When you
push a new revision of something another app depends on, the hub records that
the dependency went **stale** and notifies — it does not ship your bytes
anywhere. Consumers pull.

`type_id` is `owner-app/type-name`, kebab-case, and must be unique across the
whole platform. Dots are rejected.

```
apro-cad/mass-properties
apro-cad/reference-geometry
hexadof/model-file
propulsor/thrust-curve
```

---

## Your first integration

### 1. Say what you publish and consume

An app must declare an **interface** before anything it writes will be accepted.
Writes to undeclared types fail with `write_not_declared` — this is the guard
that stops two apps quietly colliding on the same `type_id`.

```rust
use apro_client::{AppInterface, ConsumeDecl, Mode, TypeId};

let interface = AppInterface {
    app: "apro-cad".into(),
    publishes: vec![TypeId::parse("apro-cad/mass-properties")?],
    consumes: vec![ConsumeDecl {
        type_id: TypeId::parse("hexadof/model-file")?,
        default_mode: Mode::Pinned,
    }],
};

client.declare_interface(&interface)?;
```

### 2. Publish

```rust
use apro_client::Encoding;

let type_id = TypeId::parse("apro-cad/mass-properties")?;

client.push(
    &type_id,
    "satellite-a",              // instance: which specific thing this is
    Encoding::Json,
    &serde_json::to_vec(&mass_properties)?,
)?;
```

Pushing **byte-identical** content is a no-op — the store recognises it and does
not create a new revision. Pushing different bytes appends a revision and keeps
every previous one readable forever. You never overwrite history, which is
exactly what makes staleness computable downstream.

`Encoding` is `Json`, `Protobuf`, `Blob` or `Text`. The store treats all four as
opaque bytes; the choice is a contract between you and the consumer.

### 3. Consume

```rust
use apro_client::Selector;

let type_id = TypeId::parse("apro-cad/mass-properties")?;

let Some(payload) = client.pull(&type_id, "satellite-a", Selector::Latest)? else {
    // The producer has not published yet. This is normal, not an error.
    return Ok(());
};

let props: MassProperties = serde_json::from_slice(&payload.bytes)?;
```

`Selector::Number(n)` reads one exact revision; `Selector::Latest` reads the
highest. `payload.revision_number` and `payload.content_hash` tell you precisely
which revision you got.

### 4. Declare the dependency

```rust
use apro_client::EdgeRequest;

client.register_edge(&EdgeRequest {
    consumer_app: String::new(),   // overwritten from your session
    consumer_ref: None,
    type_id: TypeId::parse("apro-cad/mass-properties")?,
    instance: "satellite-a".into(),
    mode: Mode::Pinned,
    pinned_revision_number: Some(3),
    min_revision_number: None,
})?;
```

`Mode::Pinned` is the default and the safe choice: upstream changes will never
silently alter your input. Use `Mode::Tracking` when you genuinely want to
follow the head, and `Mode::Compatible` with `min_revision_number` when you only
require a floor.

---

## Connecting to the store

If APRO Works launched your app, this is the whole connect step:

```rust
use apro_client::HttpStoreClient;

let Some(client) = HttpStoreClient::from_launch_environment()? else {
    // Not launched by APRO Works. Degrade to local-only; do not fail.
    return Ok(());
};
```

The hub passes `--apro-product-slug`, `--apro-launch-token` and
`--apro-store-endpoint` on the command line. The launch token is exchanged once
for a session token. If no credential is present the call returns `Ok(None)` —
that is a normal outcome meaning "standalone", not an error.

To connect by hand (CLI tools, tests):

```rust
use apro_client::{ClientConfig, HttpStoreClient};

let client = HttpStoreClient::connect(
    ClientConfig::new("http://127.0.0.1:5555", "my-app"),
)?;
```

`ClientError::is_unreachable()` distinguishes "the store is not running" from
"the store rejected you", so you can degrade gracefully instead of showing the
user a stack trace.

---

## Staying in sync

Cheap polling with no payload traffic:

```rust
let mut cursor = 0;

loop {
    for event in client.events(cursor)? {
        cursor = event.seq;             // monotonically increasing, never replays

        if event.type_id.as_deref() == Some("apro-cad/mass-properties") {
            let type_id = TypeId::parse("apro-cad/mass-properties")?;
            let payload = client.pull(&type_id, "satellite-a", Selector::Latest)?;
            // recompute, or simply mark your current result stale
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(2));
}
```

`events()` is the **change feed**. Reads are tracked separately — `access_log()`
records who pulled what — so the change feed is not spammed by consumers
polling. Polling is cheap by design; do not build a bespoke notify protocol on
top of it.

An `EventRecord` carries `seq`, `kind`, optional `type_id` / `artifact_id` /
`revision_id` / `edge_id`, and `summary`. It deliberately does **not** carry the
payload or the instance name — resolve those with a `pull` once you know you
care.

---

## Rules worth internalising

1. **The hub never parses your payload.** If you need a new encoding, use one
   the store already knows or agree one with the consuming app.
2. **Revisions are immutable.** "Editing" published data means pushing a new
   revision.
3. **Declare before you write.** Undeclared writes are refused, not warned.
4. **Pinned is the default.** Only opt into `Tracking` deliberately.
5. **`Ok(None)` is not a failure.** A missing upstream revision is the normal
   state before a producer has run.
6. **`type_id` is kebab-case** and namespaced by the owning app. No dots.

---

## Development

```sh
cargo test --all
```

`crates/apro-client/tests/api_surface.rs` builds the exact structs shown in this
README, so the examples cannot silently drift from the real API.

The store, the HTTP API and the hub UI live in
[APRO-works](https://github.com/APRO-pk/APRO-works). This repository is
deliberately just the app-facing half: wire types plus one transport.

There is no `LICENSE` file here — matching the other APRO repositories — so
treat it as all rights reserved until one is added.
