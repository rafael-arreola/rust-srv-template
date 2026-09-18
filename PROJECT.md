# Project Compass (`PROJECT.md`)

The domain map of this service: which modules exist, why, how they relate, and the business rules no single module can enforce alone. Technical rules live in `AGENTS.md`; a module's own intent and invariants live in the `//!` header of its `mod.rs`. Nothing here repeats either.

<!-- When cloning this boilerplate: fill §1 and §2, keep them current per §5. -->

---

## 1. Mission

- **Service:** `[SERVICE_NAME]`
- **Business purpose:** `[the problem it solves and for whom]`
- **Consumers:** `[public API, web/mobile, other services, async events, crons]`
- **Operational perimeter:** `[what it does — and what it must never do]`

---

## 2. Domain Catalog

| Module | Archetype | Intent (one line; the full contract is in its `mod.rs` header) |
| :----- | :-------- | :------------------------------------------------------------- |
| `[billing]` | `[1 · Stateful]` | `[invoices, fiscal stamping, credit notes]` |
| `[notifications]` | `[4a · Consumer]` | `[async transactional email and webhooks]` |

> The demo slices `users`, `orders`, `checkout`, `health` are live references for archetypes 1, 2, and 3. Rename, refactor, or delete them when the real domain arrives.

### 2.1 Shared Kernel (`src/shared/`)

| Type | File | Serde form | Consumers | Pure rules |
| :--- | :--- | :--------- | :-------- | :--------- |
| `Country` | `geo.rs` | ISO 3166-1 alpha-2, upper (`"MX"`) | none yet (scaffold) | `code()`, `currency()`, `TryFrom<&str>` |
| `Currency` | `money.rs` | ISO 4217, upper (`"MXN"`) | none yet; derived by `Country::currency()` | `code()`, `minor_unit_digits()` |

A type enters here only with two consumers (two modules, or a module and a vendor). If it grows a lifecycle, I/O, or persistence, it becomes a module and this table keeps just the identifier. Persisted enums are additive: never rename or remove a variant with live data.

### 2.2 Archetypes

A module ships the files of its archetype that its responsibility needs. The archetype set is the template: no ad-hoc files, and no empty placeholder to satisfy the shape. Inside each file, one family of activities end to end (`AGENTS.md` Guardrail 15).

| Archetype | Files | Example | Hard rule |
| :-------- | :---- | :------ | :-------- |
| **1 · Stateful Domain Slice** | `mod`, `controller`, `service`, `repository`, `entity`, `document`, `dto` | `users`, `orders` | Full separation triad. |
| **2 · Composite / Workflow** | `mod`, `controller`, `workflow`, `dto` | `checkout` | **FORBIDDEN:** own `document.rs` or `repository.rs`; persists through injected services. |
| **3 · Lean / Gateway** | `mod`, `controller` | `health` | May hold **one** live resource as router state; a second one requires `service.rs`. |
| **4a · Internal Consumer** | `mod`, `consumer`, `service`, `dto` | — | No HTTP surface; triggered by `broadcast`/`mpsc`. |
| **4b · HTTP Push Worker** | `mod`, `controller`, `service`, `dto` | — | Internal task token; mounted outside `/api/v1`. |
| **5 · Bulk / Ingestion Pipeline** | `mod`, `controller`, `service`, `pipeline`, `repository`, `entity`, `document`, `dto` | — | Chunking and enqueuing in `pipeline.rs`; `strategies/` for polymorphism; `csv.rs` only with an own parser. |

Trigger rule: arrives through a channel → 4a; arrives through HTTP → 4b. HTTP alone does not make a slice archetype 1 or 3. Naming: `workflow.rs`/`usecase.rs` for orchestrators, `contract.rs` for cross-module agreements, `command.rs` for private orchestration state.

### 2.3 Optional Capabilities (shipped, not wired)

Deliberate `pub` surface nobody consumes yet. Wire it or **delete it** — unused capability has no compiler pressure and rots silently.

| Capability | Where | Activation |
| :--------- | :---- | :--------- |
| JWT/JWKS auth + RBAC | `core::auth` | Composition root: crypto provider → `Arc<dyn AuthProvider>` → `auth_middleware` per prefix → `require_permission` per route (`AGENTS.md` Guardrail 17). |
| MessagePack transport | `core::http::extractors` | `MsgPack<T>` in a handler, or `Accept: application/vnd.msgpack` with `ENABLE_MSGPACK=true`. |
| Redis | `core::db::RedisProvider` | Declare `REDIS_URL`; a startup connection failure is fatal. |
| Vendor adapter | `vendors::carrier` | Build in `AppContext` with the injected `Arc<HttpClient>`; pass `Arc<CarrierClient>` to the consuming module. |
| BSON helpers | `core::db::mongo::helpers` | `ModelId` and flexible date deserializers for legacy collections with mixed types. |

### 2.4 Out of Scope (open decisions, not omissions)

Publishable API contract (OpenAPI), write idempotency (`Idempotency-Key`), an instantiated internal event bus (strategy in `AGENTS.md` Guardrail 3, primitive not shipped), RED metrics, rate and concurrency limits. Decide each one explicitly when needed and record it here.

---

## 3. Cross-Cutting Business Rules

Rules that span two or more modules, so no single `mod.rs` header can own them. **Entry condition:** if one module can enforce it alone, it does not belong here — it goes in that module's header.

| Rule | Modules involved | Enforced where |
| :--- | :--------------- | :------------- |
| `[an order is created only for an existing user]` | `[checkout, users, orders]` | `[checkout/workflow.rs, step 1]` |

---

## 4. Capability Decision Guide

Where does a new capability go?

- **Needs its own collection?** → Archetype 1.
- **Coordinates two or more existing domains?** → Archetype 2. **FORBIDDEN:** injecting one base module into another.
- **Technical endpoint, probe, or lightweight webhook?** → Archetype 3.
- **Background job, cron, or event consumer?** → 4a if triggered by a channel, 4b if by HTTP (§2.2).
- **Bulk file ingestion, chunking, queued work?** → Archetype 5.
- **External API integration?** → `src/vendors/<name>/` receiving the injected `Arc<HttpClient>`, errors mapped to `AppError`.
- **Authentication, RBAC, principal resolution?** → `core::auth` (§2.3); never a domain module.
- **A vocabulary type (closed enum, value object) used by two modules, or a module and a vendor?** → `src/shared/`, registered in §2.1. One consumer → stays in that module. Own lifecycle → it is a module.
- **A business rule that crosses modules?** → §3. Local to one module → its `mod.rs` header.

---

## 5. Maintenance

Every change travels with its knowledge, in the same delivery:

1. **Module added, removed, or re-scoped** → update §2 (and §2.2 if a new archetype appears).
2. **A module's intent or invariants change** → update the `//!` header of its `mod.rs`. `scripts/audit.sh` check 19 fails the build if a slice ships without one.
3. **A rule crosses a second module** → move it from the header to §3.
4. **Shared type, vendor, or optional capability changes** → §2.1 / §2.3.

Technical rules never land here: they go to `AGENTS.md`, and to `scripts/audit.sh` when verifiable.
