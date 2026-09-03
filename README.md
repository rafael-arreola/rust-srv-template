# Template Backend Modular — Rust

Plantilla de arquitectura backend de alto rendimiento en Rust (Axum 0.8, MongoDB 3.8, Tokio 1, OpenTelemetry) diseñada bajo el patrón **Vertical Slice / Modular Monolith (estilo NestJS)**.

Centraliza cada dominio de negocio y todos sus componentes en una única carpeta modular (`src/modules/<dominio>/`), eliminando la sobrecomplejidad de capas horizontales dispersas y el acoplamiento de estado.

---

## Características Principales

- **Arquitectura Modular Vertical:** Cada dominio (`users`, `orders`, etc.) agrupa sus DTOs, modelos de persistencia, repositorios, servicios y controladores HTTP.
- **Aislamiento de Estado en Axum:** Cada módulo gestiona su propio estado (`Router::with_state(service)`), borrando el tipo de estado a `Router<()>` al integrarse en la aplicación. Cero macros `impl_from_ref!` y cero `AppState` monolítico.
- **Inyección por Constructor:** Dependencias explícitas en `new()` validadas en tiempo de compilación.
- **Observabilidad de Producción:** OpenTelemetry con exportador a Google Cloud Trace y fallback local a logs estructurados en JSON (`CloudLoggingFormat`). Propagación de trazas W3C `traceparent` y correlación de `trace_id` en todas las respuestas.
- **Manejo Uniforme de Errores:** Envelope estándar `{ trace_id, data, cause }`. Choke point único de logging según severidad (`Info`, `Warn`, `Error`). Mensajes públicos seguros para el cliente que ocultan detalles internos.
- **Validación Automática:** Extractor `ValidatedBody<T>` que combina negociación de contenido (JSON y MessagePack) con validación declarativa mediante `validator`.
- **Soft-Delete Universal:** Manejo nativo de borrado lógico con índices idempotentes en MongoDB.
- **Graceful Shutdown & Drenado:** Soporte para probes `/healthz` (liveness) y `/readyz` (readiness) con flag de drenado antes del cierre del listener.

---

## Estructura del Proyecto

```text
rust-srv-template/
├── Cargo.toml                      # Manifiesto unificado
├── clippy.toml                     # Reglas estrictas de clippy
├── rustfmt.toml                    # Formateo oficial de Rust 2024
├── .env.example                    # Plantilla de variables de entorno
├── AGENTS.md                       # Directiva operativa para desarrolladores e IA
│
└── src/
    ├── main.rs                     # Entrypoint del servicio
    ├── app.rs                      # Composition Root y servidor HTTP
    ├── config.rs                   # Configuración y variables de entorno
    │
    ├── core/                       # Infraestructura transversal (cero lógica de negocio)
    │   ├── db/                     # Conectores MongoDB y Redis
    │   ├── error.rs                # AppError y códigos semánticos
    │   ├── response.rs             # ApiResponse, paginación y MessagePack
    │   ├── tracer.rs               # Setup de OpenTelemetry y tracing
    │   ├── tracer/format.rs        # Formateador Cloud Logging
    │   ├── http_client.rs          # Cliente HTTP instrumentado con timeouts
    │   ├── extractors.rs           # ValidatedBody<T>
    │   ├── middleware.rs           # Trace context, Request ID y timeout
    │   └── server.rs               # ServerLauncher con graceful shutdown
    │
    └── modules/                    # DOMINIOS DE NEGOCIO AUTOCONTENIDOS
        ├── mod.rs                  # Ensamblado de rutas en /api/v1
        ├── health/                 # Health checks (/healthz y /readyz)
        ├── users/                  # Módulo de usuarios (CRUD canónico)
        │   ├── dto.rs              # DTOs validados
        │   ├── model.rs            # Modelo BSON / Mongo
        │   ├── repository.rs       # Consultas e índices
        │   ├── service.rs          # Reglas de negocio
        │   ├── controller.rs       # Handlers HTTP de Axum
        │   └── mod.rs              # UsersModule y router
        └── orders/                 # Módulo con inyección inter-módulo (depende de users)
```

---

## Inicio Rápido

### 1. Variables de Entorno
Copia el archivo de ejemplo:
```bash
cp .env.example .env
```

### 2. Ejecutar en Desarrollo
```bash
cargo run
```

### 3. Verificar Calidad y Formato
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

---

## Cómo Crear un Nuevo Módulo

1. Crea la carpeta en `src/modules/<nuevo_dominio>/`.
2. Define los contratos en `dto.rs` con anotaciones `#[validate(...)]`.
3. Define la estructura de persistencia en `model.rs`.
4. Implementa las operaciones en `repository.rs` con creación idempotente de índices.
5. Implementa la lógica en `service.rs`.
6. Define las rutas en `controller.rs` usando `State(Arc<Service>)` y `ValidatedBody`.
7. Expón el constructor y router en `mod.rs`.
8. Regístralo en `src/modules/mod.rs` montándolo en el router de la API.
