# Template Backend Modular en Rust

Plantilla de microservicio en Rust organizada como un **Cargo Virtual Workspace** en dos crates (`engine/` y `service/`) bajo una arquitectura por capas con repositorios cerrados, inyección de dependencias en arranque, modelo único por entidad y sobre HTTP uniforme en JSON y MessagePack.

## Estructura del Workspace

```text
rust-srv-template/
├── Cargo.toml                  # Virtual Workspace (engine + service) y lints estrictos
├── AGENTS.md                   # Lineamientos de arquitectura y reglas verificables
├── .env.example                # Catálogo de variables de entorno del servicio
├── rust-toolchain.toml         # Versión de toolchain fijada
├── deny.toml                   # Política de supply-chain (cargo-deny)
├── scripts/audit.sh            # Batería de 24 checks arquitectónicos
├── build/
│   ├── Dockerfile              # Build multi-stage con compuertas de calidad integradas
│   └── cloudbuild.yaml         # Pipeline CI/CD para Google Cloud Run
├── engine/                     # Motor técnico reutilizable (runtime, types, stores, web)
│   └── src/
│       ├── runtime/            # Envs, Registry<T> con scopes paralelos, telemetría OTEL/GCP
│       ├── types/              # Cast<T, Db>, UtcDateTime, Context, AppError con trace hops
│       ├── stores/             # Singletons nombrados Mongo, Redis, Http y QueryResult (features)
│       └── web/                # ServerLauncher, layers, extractors, response, auth/RBAC, provide!
└── service/                    # Código del microservicio de negocio
    └── src/
        ├── main.rs             # Arranque limpio: engine::boot, Envs, Telemetry, Mongo, Redis, Http
        ├── bootstrap.rs        # Instancia *Store/*Client, verifica índices y publica *Service
        ├── envs.rs             # Struct Env: FromEnv + acceso global envs::get()
        ├── routes.rs           # Tabla única de rutas de negocio (/api/v1 y reinos de auth)
        ├── models/             # Modelos puros de dominio e invariantes (User, Order, Money, Kilograms)
        ├── stores/             # Subsets cerrados sobre Mongo::get + Collection<Document> (sin DI en new)
        ├── clients/            # Adaptadores salientes HTTP hacia proveedores externos (CarrierClient)
        ├── services/           # Lógica de negocio y agregación (*Service ensamblados por DI)
        └── http/               # *Request, *Resource, respond!, Status<T> y rutas con State<*Service>
```

## Flujo y Capas

```text
Cliente HTTP ─▶ http/ (ValidatedBody / Status<T>) ─▶ services/ (*Service DI) ─▶ stores/ (*Store / *Repository) ─▶ Mongo / Redis
                                                            │
                                                            └─▶ clients/ (HttpClient) ─▶ APIs Externas
```

- **Modelo único (`Cast<T, Db>` y `UtcDateTime`)**: cada entidad se define una sola vez en `service/src/models/`. Hacia HTTP se serializa en su tipo amigable (`String` hexadecimal, `i64`, `UUID`, ISO-8601 `Z`), y dentro de `engine::db` usa `BSON ObjectId` / `ISODate` o el tipo nativo de la BD.
- **Almacenes / Repositorios cerrados (`service/src/stores/`)**: cada `*Store` (o `*Repository`) obtiene la conexión desde el singleton `Mongo::get(...)` en `new()` y trabaja directo con la API nativa de `mongodb::Collection<Document>` (usando `encode_entity`/`decode_entity` de `engine::db`), sin aceptar ni devolver `Document`, `Bson` ni `Collection` en su API pública.
- **Inyección de dependencias (`bootstrap.rs` -> `services/` -> `http/`)**: `bootstrap.rs` ensambla los `*Service` y los expone con `engine::provide!`; los controladores en `http/` reciben únicamente su `State<*Service>`.
- **Sobre HTTP uniforme (`engine::web::response`)**:
  - Éxito (`200`, `201`, etc.): `{ "trace_id": "...", "data": ... }`
  - Paginado (`QueryResult<T>`): `{ "trace_id": "...", "data": { "items": [...], "total": 42 } }`
  - Sin contenido (`204` / `304`): cuerpo vacío
  - Descargas directas: `FileResponse` con `Content-Disposition` y `x-trace-id`
  - Error (`4xx` / `5xx`): `{ "trace_id": "...", "code": "...", "cause": "...", "details"?: ... }`

## Autenticación y Reinos

`engine::web::auth` provee autenticación JWT/JWKS (Auth0, Hydra o proveedores M2M), `TaskToken` para workers (Cloud Tasks), `DelegatedCredential` (relay) y RBAC por ruta (`RequirePermissionExt`). En `service/src/routes.rs` puedes montar múltiples reinos (`/api/v1/ui`, `/api/v1/xb`, `/api/v1/worker`) registrando cada proveedor con `Auth::register(...)` y envolviendo cada sub-router con `.with_jwt_auth(...)`, `.with_relay_auth(...)` o `.with_task_auth(...)`. En el template base los endpoints de ejemplo quedan abiertos hasta que el servicio conecte su proveedor real.

## Features de `engine`

Por defecto `service/Cargo.toml` activa `mongo` y `redis`. Si el microservicio opera sobre otro motor (como SQL Server o PostgreSQL), puedes desactivarlos en `service/Cargo.toml` (`default-features = false`) o activar `uuid` para usar `Cast<String, Uuid>` con `new_v7()`.

## Guía Rápida de Ejecución

### 1. Variables de entorno

Las variables obligatorias son `SERVICE_NAME`, `MONGO_URL` y `MONGO_DB`:

```bash
cp .env.example .env
cargo run -p service
```

### 2. Compuertas de verificación

```bash
cargo fmt --all --check
bash scripts/audit.sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Para ejecutar las pruebas de integración contra una instancia desechable de MongoDB:

```bash
MONGO_TEST_URL=mongodb://localhost:27017 cargo test -p service -- --ignored
```
