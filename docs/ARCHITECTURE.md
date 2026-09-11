# Architecture Reference

Consultative companion to `AGENTS.md`. Load this file when designing a new module, a nested submodule, a response contract, or an interaction between two domains. The binding rules live in `AGENTS.md`; this file holds the layouts, examples, and selection criteria that support them.

---

## 1. Module Archetypes

A module owns all its components under `src/modules/<module>/`. The internal file set is **not** a fixed list of six files: it follows whichever archetype matches the module's responsibility.

### Archetype 1: Stateful Domain Slice (full CRUD, persistent)

For entities with their own database collection. Implements the Canonical Separation Triad (`AGENTS.md` §1.3.1).

_Files:_ `mod.rs`, `controller.rs` (or `routes.rs`), `service.rs`, `repository.rs`, `entity.rs`, `document.rs`, `dto.rs`.
_Examples:_ `users`, `orders`.

| File            | Responsibility                                                                                                           |
| :-------------- | :----------------------------------------------------------------------------------------------------------------------- |
| `dto.rs`        | HTTP transport boundary: inputs with `validator` rules, serializable outputs, and the `From<Entity> for Output` mappers. |
| `entity.rs`     | Pure domain entity: business rules and invariants. No BSON, no MongoDB, no transport types.                              |
| `document.rs`   | Private BSON storage schema with `from_domain` / `into_domain`.                                                          |
| `repository.rs` | Collection access, idempotent index creation in `new(&db)`, soft-delete filtering.                                       |
| `service.rs`    | Semantic validation and business orchestration. Returns output DTOs.                                                     |
| `controller.rs` | Resource-relative routes, project extractors, `ApiResponse` envelope.                                                    |

### Archetype 2: Composite / Workflow Slice (multi-domain orchestrator)

For business flows coordinated across two or more base modules.

_Files:_ `mod.rs`, `controller.rs`, `workflow.rs` (or `service.rs`), `dto.rs`.
_Example:_ `checkout`.
_Strict rule:_ including `document.rs` or `repository.rs` is **FORBIDDEN**. Persistence is delegated through the injected base services.

### Archetype 3: Lean / Gateway Slice (transport, utility)

For technical endpoints, webhooks, or probes with no complex domain rules and no database of its own.

_Files:_ `mod.rs`, `controller.rs` (or `handlers.rs`).
_Example:_ `health`.

### Archetype 4: Worker / Consumer Slice (async processing)

For background tasks, cron jobs, or listeners on internal channels (`tokio::sync::broadcast`) with no HTTP surface.

_Files:_ `mod.rs`, `consumer.rs` (or `worker.rs`), `service.rs`, `dto.rs`.

### Archetype 5: Bulk / Ingestion Pipeline Slice (batch ingestion, CSV, chunking)

For modules that process large batch files (CSV/JSON), divide records into chunks, dispatch cloud/background tasks, and track session progress (e.g. `imports`, `transitions`).

_Files:_ `mod.rs`, `controller.rs`, `service.rs`, `pipeline.rs`, `csv.rs`, `repository.rs`, `entity.rs`, `document.rs`, `dto.rs`.
_Strict decomposition rule:_ CSV parsing logic belongs strictly in `csv.rs`. Chunking, payload generation, and background task enqueuing belong in `pipeline.rs`. `service.rs` acts only as a high-level orchestrator coordinating session creation and pipeline triggers, avoiding 700+ line god-files.

### Semantic naming guide

- `controller.rs` versus `handlers.rs` / `routes.rs`: use `routes.rs` or `handlers.rs` when the module is lightweight or purely functional in style.
- `service.rs` versus `workflow.rs` / `usecase.rs`: use `workflow.rs` or `usecase.rs` in orchestrating modules to denote a coordinated multi-step process.
- `dto.rs` versus `contract.rs`: use `contract.rs` when the file defines stable inter-module exchange agreements.

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

`ApiResponse::paginated(items, total, page, limit)`, returning `Paginated<T>` (alias `PaginatedResponse<T>`). `total_pages` is computed with `(total).div_ceil(limit)`:

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

**Strict isolation.** The orchestrator may call only public **service** methods and consume public **output DTOs**. Accessing repositories, MongoDB collections, or internal BSON models of the coordinated modules is **FORBIDDEN**.

**Resulting topology.** A directed acyclic graph: `Orchestrator -> A` and `Orchestrator -> B`. Modules `A` and `B` stay fully decoupled and remain unaware of each other and of the orchestrator.

### Strategy 2: Internal event channels (temporal decoupling)

**When it applies.** The interaction is reactive or a side effect (notifications, auditing, statistics recalculation, cache invalidation) where the emitter needs no immediate result and does not condition its flow on the receiver.

**Implementation.** In-memory `tokio::sync::broadcast` or `mpsc` channels created in `AppContext` or during startup. The emitter publishes; interested modules subscribe during their initialization. Zero type imports between modules.

### Strategy 3: Bounded-context merge (domain boundary re-evaluation)

**When it applies.** Two modules need continuous bidirectional calls for elementary CRUD and cannot operate independently. This is a code smell indicating false domain fragmentation.

**Implementation.** Merge both concepts into a single cohesive vertical slice instead of adding accidental complexity with artificial intermediary modules.

---

## 4. Nested Submodule Typologies

When an aggregate root groups subordinate concepts that do not justify a top-level module, nest at **exactly one level**: `src/modules/<parent>/<child>/` (`AGENTS.md` §1.3.9 and Guardrail 4).

### Typology 1: Domain Strategy Sub-Slice

For internal polymorphism, calculation engines, document generation, or export formatters. Example: `src/modules/manifests/documents/` with concrete strategies such as `manielec.rs` and `dips.rs`, plus helpers like `sanitizer.rs`.

_Structure:_ `mod.rs` defines the strategy trait or enum and its result types, alongside the concrete strategy files. It has no repository and no controller of its own; it operates on the aggregate root's entities and is consumed by the parent's service.

### Typology 2: Nested REST Resource Sub-Slice

For child entities whose lifecycle depends exclusively on the aggregate root. Example: user addresses at `src/modules/users/addresses/`.

_Structure:_ its own relative sub-router (`controller.rs`), `service.rs`, `dto.rs`, and `entity.rs`.

_Composable routing:_ the global router (`src/modules/router.rs`) mounts **only** the parent, with `.nest("/users", ...)`. The parent's `controller.rs` nests the relative sub-router, for example `.nest("/{id}/addresses", addresses_routes)`.

### Single facade rule

The parent module is always the aggregate's only public face. If an external module needs a contract from the nested submodule, the parent re-exports it explicitly in its `mod.rs`, for example `pub use documents::{DocumentPayload, ManifestDocumentType};`.

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
3. **Repository result contract:** never return untyped raw tuples `AppResult<(Vec<T>, u64)>`. Return a typed query result:
   ```rust
   pub(super) struct QueryResult<T> {
       pub items: Vec<T>,
       pub total: u64,
   }
   ```
   The service maps `QueryResult<Entity>` into the public transport `Paginated<OutputDto>`.

---

## 6. Anti-Patterns and Code Smells Catalog (The "Don'ts")

Based on real-world production drift, these patterns violate the repository architecture and must be actively avoided:

### Anti-Pattern 1: The Catch-All `model.rs`

- **Symptom:** collapsing domain entities, BSON storage documents (`*Model`), query filters, and Value Objects into a 500-line `model.rs` and making it `pub mod model;`.
- **Consequence:** breaks compiler visibility guardrails. DTOs start importing models, and external modules start mutating internal storage representations directly.
- **Correction:** strictly maintain the Canonical Separation Triad: `dto.rs` (transport), `entity.rs` (domain rules + value objects), `document.rs` (BSON schemas + mappers).

### Anti-Pattern 2: Mutable Staging Commands in `dto.rs`

- **Symptom:** creating `*Command` structs in `dto.rs` that store `raw_payload: Option<Value>`, contain domain conversion methods (`get_country()`, `to_manifest_data()`), or mutate fields across service steps.
- **Consequence:** turns DTOs into stateful staging bags, mixing HTTP transport with domain and infrastructure logic.
- **Correction:** `dto.rs` contains only immutable serialization contracts (`*Input`, `*Output`, `*Query`). Multi-step orchestration belongs in domain services or dedicated pipeline builders.

### Anti-Pattern 3: Ephemeral Lifetime Structs in Services

- **Symptom:** defining ad-hoc structs with borrowed lifetimes (such as `struct OrderAssembly<'a>`) at the top of `service.rs` merely to bundle function arguments or silence clippy.
- **Consequence:** hides poor encapsulation and creates artificial procedural glue.
- **Correction:** use typed Domain Value Objects, entity builders (`Entity::builder()`), or sub-components within the slice.

### Anti-Pattern 4: Domain Entity Leakage Across Services

- **Symptom:** public service methods returning `AppResult<Entity>` (e.g. `get_order -> AppResult<Order>`) or accepting other modules' entities.
- **Consequence:** tightly couples all modules to internal domain definitions, preventing independent domain evolution.
- **Correction:** public service methods strictly receive DTOs or primitives and return Output DTOs (`AppResult<OrderOutput>`).

---

## 7. Capability Decision Guide

The decision tree for "where does this new capability go" lives in `PROJECT.md` §4, which is the business-facing compass and the single source of truth for that question. Do not duplicate it here.
