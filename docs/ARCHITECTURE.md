# Architecture Reference

Consultative companion to `AGENTS.md`. Load this file when designing a new module, a nested submodule, a response contract, an interaction between two domains, or the composition root. The binding rules live in `AGENTS.md`; this file holds the layouts, examples, and selection criteria that support them.

---

## 1. Module Archetypes

A module owns all its components under `src/modules/<module>/`. The internal file set is **not** a fixed list of six files: it follows whichever archetype matches the module's responsibility.

### Archetype 1: Stateful Domain Slice (full CRUD, persistent)

For entities with their own database collection. Implements the Canonical Separation Triad (`AGENTS.md` §1.3.1).

_Files:_ `mod.rs`, `controller.rs` (or `routes.rs`), `service.rs`, `repository.rs`, `entity.rs` (or directory `entity/`), `document.rs` (or directory `document/`), `dto.rs` (or directory `dto/`).
_Examples:_ `users`, `orders`.

| File                 | Responsibility                                                                                                           |
| :------------------- | :----------------------------------------------------------------------------------------------------------------------- |
| `dto.rs` (or `dto/`) | HTTP transport boundary: inputs with `validator` rules, serializable outputs, and the `From<Entity> for Output` mappers. |
| `entity.rs`          | Pure domain entity: business rules and invariants. No BSON, no MongoDB, no transport types.                              |
| `document.rs`        | Private BSON storage schema with `from_domain` / `into_domain`.                                                          |
| `repository.rs`      | Collection access, idempotent index creation in `new(&db)`, soft-delete filtering.                                       |
| `service.rs`         | Semantic validation and business orchestration. Returns output DTOs.                                                     |
| `controller.rs`      | Resource-relative routes, project extractors, `ApiResponse` envelope.                                                    |

Add `command.rs` (`pub(super)`) when a flow needs internal orchestration state, and split `repository.rs`/`dto/` per §8 when the slice grows.

### Archetype 2: Composite / Workflow Slice (multi-domain orchestrator)

For business flows coordinated across two or more base modules.

_Files:_ `mod.rs`, `controller.rs`, `workflow.rs` (or `service.rs`), `dto.rs`.
_Example:_ `checkout`.
_Strict rule:_ including `document.rs` or `repository.rs` is **FORBIDDEN**. Persistence is delegated through the injected base services. The orchestrator accesses neither the base modules' repositories/documents nor their entities, and never writes their persistence.

### Archetype 3: Lean / Gateway Slice (transport, utility)

For technical endpoints, webhooks, or probes with no complex domain rules and no database of its own.

_Files:_ `mod.rs`, `controller.rs` (or `handlers.rs`).
_Example:_ `health`.

A Lean slice may hold a single injected live resource (for example the MongoDB handle) as erased router state via `.with_state(resource)`; this is not the monolithic `AppState` that `AGENTS.md` Guardrail 10 forbids. A slice holds at most one resource this way; anything richer becomes a `service.rs` state.

### Archetype 4a: Internal Consumer Slice (async, no HTTP surface)

For background tasks, cron jobs, or reactive listeners on internal channels (`tokio::sync::broadcast` / `mpsc`). It emits and consumes typed events and exposes no route of its own.

_Files:_ `mod.rs`, `consumer.rs` (or `worker.rs`), `service.rs`, `dto.rs`.

### Archetype 4b: HTTP Push Worker Slice (Cloud Tasks / provider webhook)

For tasks delivered over HTTP by an external scheduler (Cloud Tasks, Cloud Scheduler) or a provider webhook. It **does** expose a `controller.rs`, mounted on its own prefix and protected by an internal task token instead of user authentication.

_Files:_ `mod.rs`, `controller.rs`, `service.rs`, `dto.rs`.
_Strict isolation:_ the endpoint validates the internal token through a dedicated `core` middleware/extractor (see §6), lives outside the public `/api/v1` surface, and delegates any persistence to the services it coordinates.

> Rule of thumb: if the trigger arrives through a channel, it is 4a; if it arrives through an HTTP request, it is 4b. HTTP presence alone does not make a slice "Archetype 1" or "Archetype 3".

### Archetype 5: Bulk / Ingestion Pipeline Slice (batch ingestion, chunking)

For modules that process large batch files (CSV/JSON), divide records into chunks, dispatch cloud/background tasks, and track session progress (e.g. `imports`, `transitions`).

_Files:_ `mod.rs`, `controller.rs`, `service.rs`, `pipeline.rs`, `repository.rs`, `entity.rs`, `document.rs`, `dto.rs`; add `strategies/` for internal polymorphism.

_Decomposition rule:_ chunking, payload generation, and background task enqueuing belong in `pipeline.rs`. `strategies/` is valid and expected when the slice formats or transforms records polymorphically. **`csv.rs` is optional**: create it only when the slice ships its own parser; if parsing is delegated to a vendor or existing utility, do not add the file. `service.rs` acts only as a high-level orchestrator coordinating session creation and pipeline triggers, never as a god-file.

### Semantic naming guide

- `controller.rs` versus `handlers.rs` / `routes.rs`: use `routes.rs` or `handlers.rs` when the module is lightweight or purely functional in style.
- `service.rs` versus `workflow.rs` / `usecase.rs`: use `workflow.rs` or `usecase.rs` in orchestrating modules to denote a coordinated multi-step process.
- `dto.rs` versus `contract.rs`: use `contract.rs` when the file defines stable inter-module exchange agreements.
- `command.rs`: internal orchestration commands and multi-step pipeline state, private to the slice (`pub(super)`).

---

## 2. Canonical Response Shapes

Every API response is one of four `ApiResponse<T>` shapes (`AGENTS.md` §1.3.4).

### Single success

`ApiResponse::ok(data)` or `ApiResponse::created(data)`, HTTP 200 or 201:

```json
{
  "trace_id": "018f3a5b6c7d8e9f...",
  "data": { "id": "...", "name": "..." }
}
```

### Paginated success

`ApiResponse::paginated(items, total, page, limit)`, returning `Paginated<T>`. `total_pages` is computed with `(total).div_ceil(limit)`. Pagination is always `page`/`limit: u64`.

```json
{
  "trace_id": "018f3a5b6c7d8e9f...",
  "data": {
    "data": [{ "...": "..." }],
    "total": 100,
    "total_pages": 5,
    "page": 1,
    "limit": 20
  }
}
```

### No content

`ApiResponse::empty()`, HTTP 204, no serialized body.

### Controlled error

Returned by `Err(AppError::...)`; `IntoResponse` formats the envelope and hides infrastructure detail. HTTP status matches the error kind (400, 404, 409, 422, 500, and so on):

```json
{
  "trace_id": "018f3a5b6c7d8e9f...",
  "data": {
    "message": "User not found: 64f1a2...",
    "breadcrumbs": [
      "User not found: 64f1a2...",
      "Client verification failed",
      "Checkout process failed"
    ]
  },
  "cause": "NOT_FOUND"
}
```

`breadcrumbs` is optional inside `data`: it appears only when public context was stacked with `.attach_public(...)`. Without it, the field is omitted and the minimal shape is preserved.

---

## 3. Inter-Module Decoupling Strategies

When two domains must interact, pick exactly one strategy based on business semantics. Cycles and deferred mutable injection are forbidden outright (`AGENTS.md` Guardrail 3).

### Strategy 1: Orchestrator module (recommended for synchronous flows)

**When it applies.** A coordinated business operation needs synchronism and an immediate response within the HTTP request, involving two or more domains. Example: a checkout flow that validates a user and then creates an order.

**Implementation.** Create an independent vertical module in `src/modules/<orchestrator>/` whose `new()` receives the concrete services `Arc<AService>` and `Arc<BService>`.

**Strict isolation.** The orchestrator may call only public **service** methods and consume public **output DTOs**. Accessing repositories, MongoDB collections, BSON models, or domain entities of the coordinated modules is **FORBIDDEN**.

**Resulting topology.** A directed acyclic graph: `Orchestrator -> A` and `Orchestrator -> B`. Modules `A` and `B` stay fully decoupled and remain unaware of each other and of the orchestrator.

### Strategy 2: Internal event channels (temporal decoupling)

**When it applies.** The interaction is reactive or a side effect (notifications, auditing, statistics recalculation, cache invalidation) where the emitter needs no immediate result and does not condition its flow on the receiver. Pairs with Archetype 4a.

**Implementation.** In-memory `tokio::sync::broadcast` or `mpsc` channels created in `AppContext` or during startup. The emitter publishes; interested modules subscribe during their initialization. Zero type imports between modules.

### Strategy 3: Bounded-context merge (domain boundary re-evaluation)

**When it applies.** Two modules need continuous bidirectional calls for elementary CRUD and cannot operate independently. This is a code smell indicating false domain fragmentation.

**Implementation.** Merge both concepts into a single cohesive vertical slice instead of adding accidental complexity with artificial intermediary modules.

---

## 4. Nested Submodule Typologies

When an aggregate root groups subordinate concepts that do not justify a top-level module, nest at **exactly one level**: `src/modules/<parent>/<child>/` (`AGENTS.md` §1.3.8 and Guardrail 4).

### Typology 1: Domain Strategy Sub-Slice

For internal polymorphism, calculation engines, document generation, or export formatters. Example: `src/modules/manifests/documents/` with concrete strategies such as `manielec.rs` and `dips.rs`, plus helpers like `sanitizer.rs`.

_Structure:_ `mod.rs` defines the strategy trait or enum and its result types, alongside the concrete strategy files. It has no repository and no controller of its own; it operates on the aggregate root's entities and is consumed by the parent's service.

### Typology 2: Nested REST Resource Sub-Slice

For child entities whose lifecycle depends exclusively on the aggregate root. Example: user addresses at `src/modules/users/addresses/`.

_Structure:_ its own relative sub-router (`controller.rs`), `service.rs`, `dto.rs`, and `entity.rs`.

_Composable routing:_ the global router (`src/modules/router.rs`) mounts **only** the parent, with `.nest("/users", ...)`. The parent's `controller.rs` nests the relative sub-router, for example `.nest("/{id}/addresses", addresses_routes)`.

### Single facade rule

The parent module is always the aggregate's only public face. If an external module needs a contract from the nested submodule, the parent re-exports it explicitly in its `mod.rs`, for example `pub use documents::{DocumentPayload, ManifestDocumentType};`.

Re-exports are limited to explicit contracts. `pub use entity::...` or `pub use repository::...` in `mod.rs` is **FORBIDDEN** because it annuls the `pub(super)` compiler guardrail; cross-module projections are exposed through a dedicated `contract.rs`.

---

## 5. Query Criteria and Repository Results Pattern

Multi-field search filters and database criteria must never leak HTTP or MongoDB coupling across boundaries.

### Query criteria placement

1. **HTTP boundary (`dto.rs`):** declare a validated `List<Resource>Query` with transport types and pagination:
   ```rust
   #[derive(Debug, Deserialize, Validate)]
   pub struct ListOrdersQuery {
       pub country: Option<String>,
       pub status: Option<i64>,
       pub search: Option<String>,
       #[serde(flatten)]
       pub pagination: PaginationQuery,
   }
   ```
2. **Repository criteria (`repository.rs` or private `criteria.rs`):** declare strongly-typed criteria used by queries:
   ```rust
   pub(super) struct OrderCriteria {
       pub country: Option<Country>,
       pub status: Option<i64>,
       pub search: Option<String>,
       pub skip: u64,
       pub limit: i64,
   }
   ```
3. **Repository result contract:** never return untyped raw tuples `AppResult<(Vec<T>, u64)>`. `QueryResult<T>` is a core persistence type in `crate::core::db` (not `core::http`) and is reused by repositories:

   ```rust
   use crate::core::db::QueryResult;

   pub(super) async fn list(&self, criteria: OrderCriteria) -> AppResult<QueryResult<Order>> { ... }
   ```

   `QueryResult<T>` carries `{ items: Vec<T>, total: u64 }`. The service maps `QueryResult<Entity>` into the public transport `Paginated<OutputDto>`. `Paginated<T>` is an HTTP transport shape and must never cross the repository boundary.

---

## 6. Request Context & Authentication

Authentication and RBAC are a `core` capability, never a domain module.

### `core::auth` surface (shipped, not wired)

The template ships this capability **disconnected**: no module or router uses it until you wire it. Public surface: `AuthProvider` (port), `AuthPrincipal` (with a typed `FromRequestParts` extractor), `JwtJwksAuthProvider`/`JwtJwksClaims`, `auth_middleware`, `task_auth_middleware`, `TaskToken`, `require_permission`, `require_any_permission`, and `extract_bearer_token`.

Wiring steps (composition root only):

1. Install the `jsonwebtoken` crypto provider once at startup, before constructing any provider.
2. Build `Arc<dyn AuthProvider>`, for example `JwtJwksAuthProvider::new(jwks_url, allowed_client_ids).await?`.
3. Layer `auth_middleware` on the protected prefix: `.layer(from_fn_with_state(provider, auth_middleware))`.
4. Apply `require_permission`/`require_any_permission` per route; controllers read `AuthPrincipal` through the extractor.

Unwired `core::auth` items are reachable `pub` surface (no `dead_code` by design), so the capability is opt-in with zero effect until step 2 exists.

### Request pipeline

1. An inbound middleware validates the credentials and injects the resolved **principal** (identity plus roles) and the operational **country** into the request context.
2. The `controller.rs` extracts them with typed extractors from `core::http::extractors` (alongside `body.rs`, `query.rs`, `path.rs`, `msgpack.rs`).
3. The controller passes **primitives** (for example `user_id: String`, `country: Country`) to the service.
4. The service **NEVER** reads `extensions`: it receives everything it needs as explicit arguments.

### RBAC

Route-level authorization belongs to `core` middleware/router layers. A domain module consumes the principal through an extractor; it never parses raw headers, resolves tokens, or implements authorization rules.

### Internal task token (Archetype 4b)

HTTP push workers are protected by an internal task token validated by a dedicated `core` middleware/extractor. This is not user authentication: it authorizes the scheduler/webhook caller and never lives on the public `/api/v1` prefix (see §9).

### Forbidden

- Reading `extensions` from a service.
- Parsing `Authorization` or equivalent headers inside `controller.rs` or a domain module.
- Placing authentication, RBAC, or principal types in `src/modules/`.
- Generating or propagating `x-request-id`; correlation is W3C `traceparent`/`trace_id` only.

---

## 7. Composition Root & DI

The composition root (`src/app.rs` plus `src/main.rs`) owns wiring; it contains zero business logic.

### What lives in `AppContext`

`AppContext` holds only **live resources**:

- `db: Database` (MongoDB pool).
- `redis: Option<RedisProvider>` (present only when `REDIS_URL` is declared).
- `http: Arc<HttpClient>` (the single instrumented client).

Configuration is not stored in `AppContext`: it is read through `config::get()`. Injecting `&Env` into modules is **FORBIDDEN**.

### Startup order

1. `config` load with `require_env`/`require_env_parsed` (fail-fast; optional defaults only through a visible helper).
2. Single rustls crypto provider installed in `main`; minimal Tokio feature set.
3. Telemetry initialization.
4. MongoDB connection pool.
5. Optional Redis: if declared, a connection failure is fatal (`ExitCode::FAILURE`).
6. `HttpClient::try_new() -> AppResult<Self>`; a construction failure is fatal and never degraded to a client without timeouts.
7. `AppContext` assembled with the live resources above.
8. Vendors constructed, each receiving the shared `Arc<HttpClient>`.
9. Modules resolved in `Modules::init(&ctx)` (base leaves before orchestrators).
10. Router tree composed and fallbacks registered last.

Any failure in this sequence returns `ExitCode::FAILURE`; it is never logged and swallowed as a successful start.

### Dependency injection style

- Constructor injection in `new()` with concrete `Arc<T>`; `Arc<dyn Trait>` only inside the closed exception list (`AGENTS.md` Guardrail 11).
- When a slice's `new()` exceeds roughly four dependency arguments, group them in an owned `XModuleDeps` struct (live resources/services only) instead of `#[allow(clippy::too_many_arguments)]`.
- Vendors are built before modules; modules receive `Arc<Vendor>` and `Arc<OtherService>` explicitly.

---

## 8. Large Slice Decomposition

The ~400-line threshold is **universal** to every file inside a slice (`service.rs`, `controller.rs`, `repository.rs`, `command.rs`, and so on), not exclusive to ingestion services. When a file approaches it, split by responsibility inside the same slice:

| Pattern                                  | Use it for                                                                     |
| :--------------------------------------- | :----------------------------------------------------------------------------- |
| `command.rs`                             | Internal orchestration commands and multi-step pipeline state (`pub(super)`).  |
| `pipeline.rs`                            | Multi-step flows: chunking, payload generation, task enqueuing.                |
| `strategies/`                            | Internal polymorphism, formatters, calculation/export engines.                 |
| `repository/{queries,mutations,bulk}.rs` | Splitting read, write, and batch persistence concerns.                         |
| `dto/` (segmented directory)             | Large transport contracts (`*Input`, `*Output`, `*Query`) grouped by resource. |
| `entity/` (optional directory)           | Aggregates with rich invariants, subordinate child entities, or multiple VOs.  |
| `document/` (optional directory)         | Complex BSON schemas with heavy bidirectional mappers (`mappers.rs`, etc.).    |
| `csv.rs`                                 | Only when the slice ships its own parser. Optional, never promised.            |

`service.rs` stays a thin orchestration facade. `dto.rs` and `dto/` contain only immutable serialization contracts; orchestration state belongs in `command.rs`.

### DTO Directory Taxonomy (`src/modules/<name>/dto/`)

When a slice handles standard CRUD, multiple query filters, or sub-resources, decompose `dto.rs` into a dedicated directory:

- `mod.rs`: Re-exports all contracts cleanly (`pub use input::*; pub use output::*; pub use query::*;`).
- `input.rs`: Inbound mutation payloads (`*Input`) deriving `Deserialize` and `Validate`.
- `output.rs`: Outbound projection payloads (`*Output`) deriving `Serialize` and projection mappers (`impl From<Entity> for Output`).
- `query.rs`: URL query string parameters (`*Query`), re-exporting or flattening `PaginationQuery`.
- Sub-resource attachments (`items.rs`, `lines.rs`, `detail.rs`, `summary.rs`): Subordinate transport structures that prevent `input.rs` and `output.rs` from becoming monolithic.

### Entity Directory Taxonomy (`src/modules/<name>/entity/`)

When an aggregate root accumulates multiple Value Objects, validation rules, or subordinate child entities, decompose `entity.rs` into a dedicated directory:

- `mod.rs`: Re-exports the aggregate root and Value Objects (`pub(super) mod entity;` in the slice's `mod.rs`).
- `root.rs`: The Aggregate Root struct and primary business methods.
- `items.rs` (or `<child>.rs`): Subordinate domain entities owned exclusively by the aggregate root.
- `value_objects.rs`: Pure domain Value Objects with localized invariants.

**Compiler invariant:** In `src/modules/<name>/mod.rs`, declare `pub(super) mod entity;`. The directory remains fully private to the slice and invisible to other modules.

### Document Directory Taxonomy (`src/modules/<name>/document/`)

When a MongoDB document schema grows with embedded sub-documents and extensive bidirectional mapping logic, decompose `document.rs` into a dedicated directory:

- `mod.rs`: Re-exports the primary document schema and mappers (`pub(super) mod document;` in the slice's `mod.rs`).
- `schema.rs`: BSON storage struct (`ObjectId`, `_id`, `deleted_at`).
- `mappers.rs`: Bidirectional transformation functions (`from_domain` and `into_domain`).
- `subdocuments.rs`: Embedded BSON representations for subordinate child entities.

**Compiler invariant:** In `src/modules/<name>/mod.rs`, declare `pub(super) mod document;`. The private storage schema remains protected against cross-module imports (`error[E0603]`).

---

## 9. Multi-Router Composition

A service may expose several orthogonal prefixes with different policies:

- `/api/v1`: public API surface, user authentication and RBAC.
- `/internal/tasks`: HTTP push workers (Archetype 4b), internal task token, no user auth.
- `/`: probes and technical endpoints (Archetype 3).

Composition rules:

1. `src/modules/router.rs` owns prefix composition; `controller.rs` declares only resource-relative routes (`/`, `/{id}`).
2. Each prefix is assembled from its modules' routers; policies (auth, token, rate/size limits) are attached per prefix, not per module.
3. Subroutes of an aggregate belong to the aggregate's owning module. For example, `/{id}/manifests` is owned and merged by `manifests`, never patched from a leaf module that merely consumes it.
4. The composition root registers `fallback::not_found` and `method_not_allowed_fallback` **after** the whole tree is mounted, so every 404/405 stays inside the uniform envelope.

---

## 10. Anti-Patterns and Code Smells Catalog (The "Don'ts")

Based on real-world production drift, these patterns violate the repository architecture and must be actively avoided:

### Anti-Pattern 1: The Catch-All `model.rs`

- **Symptom:** collapsing domain entities, BSON storage documents (`*Model`), query filters, and Value Objects into a 500-line `model.rs` and making it `pub mod model;`.
- **Consequence:** breaks compiler visibility guardrails. DTOs start importing models, and external modules start mutating internal storage representations directly.
- **Correction:** strictly maintain the Canonical Separation Triad: `dto.rs` (transport), `entity.rs` (domain rules + value objects), `document.rs` (BSON schemas + mappers).

### Anti-Pattern 2: Mutable Staging Commands in `dto.rs`

- **Symptom:** creating `*Command` structs in `dto.rs` that store `raw_payload: Option<Value>`, contain domain conversion methods (`get_country()`, `to_manifest_data()`), or mutate fields across service steps.
- **Consequence:** turns DTOs into stateful staging bags, mixing HTTP transport with domain and infrastructure logic.
- **Correction:** `dto.rs` contains only immutable serialization contracts (`*Input`, `*Output`, `*Query`). Multi-step orchestration belongs in a private `command.rs` or a dedicated pipeline builder.

### Anti-Pattern 3: Ephemeral Lifetime Structs in Services

- **Symptom:** defining ad-hoc structs with borrowed lifetimes (such as `struct OrderAssembly<'a>`) at the top of `service.rs` merely to bundle function arguments or silence clippy.
- **Consequence:** hides poor encapsulation and creates artificial procedural glue.
- **Correction:** use typed Domain Value Objects, entity builders (`Entity::builder()`), owned `XModuleDeps` for constructor dependencies, or sub-components within the slice.

### Anti-Pattern 4: Domain Entity Leakage Across Services

- **Symptom:** public service methods returning `AppResult<Entity>` (e.g. `get_order -> AppResult<Order>`) or accepting other modules' entities.
- **Consequence:** tightly couples all modules to internal domain definitions, preventing independent domain evolution.
- **Correction:** public service methods strictly receive DTOs or primitives and return Output DTOs (`AppResult<OrderOutput>`).

---

## 11. Capability Decision Guide

The decision tree for "where does this new capability go" lives in `PROJECT.md` §4, which is the business-facing compass and the single source of truth for that question. Do not duplicate it here.
