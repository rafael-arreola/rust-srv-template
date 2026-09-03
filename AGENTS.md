# Directiva Operativa del Repositorio (`AGENTS.md`)

Este documento constituye la **fuente de verdad técnica, arquitectónica y operativa** para cualquier agente de IA o desarrollador que opere en `rust-srv-template`.

Estructurado bajo los **tres pilares fundamentales de diseño de instrucciones modernas** (evitando el *hobbling* o micromanagement paso a paso para permitir el máximo razonamiento del modelo):
1. **Definición del Sistema y Arquitectura (El "Qué"):** Objetivos de alto nivel, contratos de tipos y arquitectura de la solución.
2. **Guardrails y Límites Infranqueables:** Fronteras estrictas de contención y seguridad dentro de las cuales el agente tiene total libertad de razonamiento.
3. **Criterios de Término Verificables (Definition of Done & Self-Verification):** Protocolos y herramientas deterministas para que el propio agente auto-verifique empíricamente su trabajo antes de darlo por concluido.

---

## Pilar 1: Definición del Sistema y Arquitectura (El "Qué")

### 1.1 Propósito y Stack del Repositorio
`rust-srv-template` es un servicio backend de producción de alto rendimiento y baja latencia en Rust, diseñado bajo el patrón **Vertical Slice / Modular Monolith (estilo NestJS en Rust)**.
- **Lenguaje y Edición:** Rust 2024.
- **Servidor Web y Runtime:** Axum 0.8, Tokio 1 (full), Tower-HTTP 0.7 (CORS, compresión y descompresión gzip).
- **Persistencia:** MongoDB 3.8 (driver oficial con instrumentación OpenTelemetry y BSON 3), Redis (opcional).
- **Observabilidad:** OpenTelemetry 0.32 con exportador a Google Cloud Trace y fallback local estructurado a Cloud Logging JSON.
- **Validación y Serialización:** `validator` 0.21, `serde`, `serde_json`, `rmp-serde` (MessagePack).
- **Manejo de Errores y Utilidades:** `thiserror` 2, `chrono` 0.4, `uuid` 1 (v7), `dotenvy` 0.15.

### 1.2 Taxonomía y Organización de Directorios
```text
src/
├── main.rs                 # Entrypoint: Inicializa crypto provider, telemetría y conexión base a BD
├── app.rs                  # Composition Root: AppContext, orquesta dependencias y monta routers
├── config.rs               # Carga inmutable y tipada de entorno (.env vía OnceLock, fail-fast)
│
├── core/                   # Capacidades técnicas transversales compartidas (CERO lógica de negocio)
│   ├── mod.rs              # Re-exports públicos limpios
│   ├── db/                 # Conectores y pools (MongoDB con helpers BSON resilientes, Redis)
│   ├── pagination.rs       # PaginationQuery universal (skip, limit, page, bounds)
│   ├── macros.rs           # as_json! para estructuración segura de logs en Cloud Logging
│   ├── error.rs            # AppError (códigos estables, severidad, IntoResponse público-seguro)
│   ├── response.rs         # ApiResponse<T>, paginación y negociación MessagePack
│   ├── tracer.rs           # OpenTelemetry (GCP Cloud Trace y fallback local estructurado)
│   ├── tracer/format.rs    # Formateador Cloud Logging JSON
│   ├── http_client.rs      # Cliente reqwest instrumentado con W3C traceparent y timeouts
│   ├── extractors.rs       # ValidatedBody<T> (deserialización + validación declarativa con validator)
│   ├── middleware.rs       # Tracing W3C/GCP, Request ID (UUIDv7) y timeout por petición
│   └── server.rs           # ServerLauncher con graceful shutdown y drenado
│
└── modules/                # MÓDULOS DE DOMINIO AUTOCONTENIDOS (Vertical Slices)
    ├── mod.rs              # Ensamblado topológico de módulos en /api/v1
    ├── health/             # Probes /healthz y /readyz con verificación de drenado
    ├── users/              # Módulo canónico CRUD (dto, model, repository, service, controller, mod)
    └── orders/             # Módulo dependiente (ilustra consumo inter-módulo de users)
```

### 1.3 Contratos Arquitectónicos Esenciales
1. **Vertical Slices Cohesivos:** Cada dominio (`users`, `orders`, etc.) centraliza todos sus componentes en su propia carpeta: DTOs (`dto.rs`), persistencia BSON (`model.rs`), queries e índices (`repository.rs`), reglas de negocio (`service.rs`), handlers HTTP (`controller.rs`) y exportador del módulo (`mod.rs`).
2. **Aislamiento de Estado en Axum:** Cada módulo encapsula su propio estado (`Router::with_state(service)`). Al montarse en el router principal (`modules/mod.rs`), su tipo de estado se borra a `Router<()>`. No existe un `AppState` global monolítico.
3. **Inyección por Constructor:** Las dependencias se inyectan explícitamente en `new()` mediante referencias `Arc<T>`.
4. **Envelope Uniforme:** Toda respuesta HTTP (éxito o error) debe cumplir con la estructura `{ trace_id, data, cause }`.
5. **Validación en Fronteras:** La validación sintáctica de peticiones se delega a `ValidatedBody<T>` en los DTOs. La validación semántica se realiza en los servicios de negocio.
6. **Observabilidad Unificada:** Todo endpoint y caso de uso debe ser trazable de extremo a extremo. El `trace_id` de OpenTelemetry generado en el middleware debe correlacionar la petición HTTP entrante, las queries de MongoDB y el envelope final de respuesta.

---

## Pilar 2: Guardrails y Límites Infranqueables

Estos límites son absolutos. El agente tiene plena autonomía para diseñar e implementar la mejor solución técnica siempre que **no traspase ninguno de estos límites**:

### 1. Límite de Dominio y Cohesión
- **PROHIBIDO** reintroducir capas horizontales globales (`src/domain/`, `src/application/`, `src/infrastructure/`). Toda lógica de un dominio pertenece a `src/modules/<dominio>/`.
- **PROHIBIDO** colocar lógica de negocio o entidades de dominio dentro de `src/core/`. `src/core/` es exclusivamente infraestructura técnica transversal y agnóstica de negocio.

### 2. Límite de Encapsulamiento de Persistencia y Fronteras de Tipos
- **PROHIBIDO** exportar o compartir modelos de base de datos BSON (`*Model`) entre módulos distintos. `UserModel` (con sus `ObjectId`, campos BSON y `deleted_at`) es un detalle de implementación privado de `modules/users/`.
- La comunicación entre módulos se realiza exclusivamente invocando el **Servicio** del otro módulo (`Arc<OtherService>`), el cual intercambia únicamente contratos públicos de salida (**DTOs** como `UserOutput`).

### 3. Límite de Dependencias Cíclicas
- **PROHIBIDO** crear dependencias circulares entre módulos (`Módulo A` $\leftrightarrow$ `Módulo B`).
- Si dos módulos requieren interacción mutua, el ciclo **debe romperse** mediante:
  - Comunicación asíncrona por eventos internos (`tokio::sync::broadcast` o `mpsc`).
  - Un servicio orquestador superior (por ejemplo, en un módulo coordinador o en `app.rs`).

### 4. Límite de Persistencia, Concurrencia y Soft-Delete (MongoDB)
- En entidades con borrado lógico (`deleted_at`):
  - **PROHIBIDO** ejecutar borrado físico (`delete_one`, `delete_many` o `find_one_and_delete`).
  - **OBLIGATORIO** filtrar `{ "deleted_at": { "$exists": false } }` en toda consulta de búsqueda, listado, conteo y actualización.
- **Creación de Índices:**
  - **PROHIBIDO** llamar a `create_indexes()` en `main.rs` o de forma desordenada. La creación idempotente de índices debe estar encapsulada privadamente en `Repository::new(&db).await`.
  - Los índices compuestos para colecciones con soft-delete deben iniciar siempre con `deleted_at: 1` (ej. `{ "deleted_at": 1, "created_at": -1 }`).
- **Concurrencia y Atomicidad:**
  - **PROHIBIDO** el antipatrón de "lectura previa y escritura posterior" para actualizaciones condicionales de estado o balance (race conditions / TOCTOU). Se deben usar operadores atómicos de MongoDB (`$set`, `$inc`, `$push`) combinados con filtros condicionales (ej. `{ "_id": oid, "stock": { "$gte": qty } }`) en un único `update_one`.

### 5. Límite de Red, Timeouts y Clientes HTTP
- **Inbound (Peticiones Entrantes):** Todo endpoint está protegido por el middleware `core::middleware::request_timeout` configurado por `REQUEST_TIMEOUT_SECS` (default 30s).
  - **PROHIBIDO** el uso de `tower_http::timeout::TimeoutLayer` porque responde con un 408 plano sin el envelope estándar del sistema.
- **Outbound (Peticiones Salientes):**
  - **PROHIBIDO** instanciar clientes directos con `reqwest::Client::new()` o `reqwest::ClientBuilder` (no tienen timeout por defecto y pueden congelar hilos asíncronos indefinidamente).
  - **OBLIGATORIO** usar `core::http_client::instrumented_client()` o `client_with_timeout()`, los cuales fuerzan 10s de timeout global, 3s de conexión TCP y propagación del header W3C `traceparent`.

### 6. Límite de Seguridad y Exposición de Errores
- **PROHIBIDO** fugar información interna de infraestructura hacia el cliente en respuestas HTTP. Cadenas de conexión de base de datos, direcciones IP internas, errores de red y stack traces deben quedar confinados en los logs internos correlacionados con el `trace_id`. El cliente solo recibe `AppError::public_message()`.
- **PROHIBIDO** el uso de `unwrap()` y `expect()` en código de aplicación (restringido por lints de clippy). Todo error debe propagarse con `?` y mapearse a `AppError`.
- **Doble Vista de Errores:**
  - `Display` / `to_string()` es exclusivamente para el log interno detallado.
  - `public_message()` es la única cadena de texto autorizada para enviarse en el payload de respuesta.

### 7. Límite de Observabilidad y Tracing
- **OBLIGATORIO** usar `#[tracing::instrument(skip_all, fields(...))]` al instrumentar funciones. **PROHIBIDO** el uso de `#[instrument]` sin `skip_all` en métodos que reciban DTOs, entidades, conexiones o dependencias para evitar desbordamiento de memoria y serialización innecesaria con `Debug`.
- **PROHIBIDO** fugar información sensible (contraseñas, tokens, datos de tarjetas, PII no anonimizada) en los campos declarados del span (`fields(...)`) o en los eventos de log.
- **PROHIBIDO** la amplificación de logs (doble/triple logging). Si una función retorna `Result<T, AppError>`, el error se propaga con `?`. El log del error se realiza **una única vez** en el choke-point de salida HTTP (`AppError::into_response`), jamás en el repositorio ni en el servicio intermedio.
- **OBLIGATORIO** usar el prefijo `%` (Display) en identificadores de búsqueda en spans (ej. `fields(%id, %email)` o `fields(user_id = %id)`).
- **OBLIGATORIO** usar la macro `crate::as_json!(&data)` al registrar payloads u objetos estructurados en eventos de log para evitar pánicos por serialización y garantizar JSON nativo en Cloud Logging.
- **OBLIGATORIO** encadenar `.instrument(tracing::Span::current())` en cualquier tarea asíncrona desacoplada (`tokio::spawn`) para garantizar la continuidad del `trace_id`.

### 8. Límite de Sintaxis y Routing Web
- **Rutas en Axum:** Axum utiliza llaves `{id}`, **PROHIBIDO** el uso de sintaxis de dos puntos (`:id`).

### 9. Límite de Economía de Código (Cero Dogma)
- **PROHIBIDO** inventar traits dinámicos (`Arc<dyn Trait>`) o dobles de prueba sintéticos (mocks) por defecto. Usa tipos concretos `Arc<T>` a menos que exista una necesidad técnica real e ineludible de polimorfismo dinámico en runtime.
- **PROHIBIDO** crear archivos de prueba o tests sintéticos no solicitados expresamente por el usuario.

---

## Pilar 3: Criterios de Término Verificables (Definition of Done)

El agente es responsable de **auto-verificar su trabajo empíricamente** antes de dar cualquier tarea por concluida. No se aceptan supuestos ni declaraciones de éxito sin ejecución previa de las herramientas oficiales del repositorio.

### 3.1 Bucle de Auto-Verificación Obligatorio
Toda tarea se considera completada única y exclusivamente si los siguientes 4 comandos se ejecutan y devuelven código de salida `0`:

```bash
# 1. Verificación oficial de formateo (Rust 2024)
cargo fmt --check

# 2. Verificación estricta del linter (CERO advertencias permitidas)
cargo clippy -- -D warnings

# 3. Verificación de compilación de tipos
cargo check

# 4. Verificación de integridad de la suite de pruebas
cargo test
```

### 3.2 Batería de Auto-Auditoría de Invariantes (Inspección Autónoma con Grep)
El agente puede ejecutar estas verificaciones rápidas para asegurar el cumplimiento de guardrails:

```bash
# 1. Verificar ausencia de unwrap/expect en código de app
cargo clippy --all-targets -- -D warnings 2>&1 | grep -i "unwrap\|expect"

# 2. Verificar que no haya borrado físico en Mongo
grep -rn "delete_one\|delete_many\|find_one_and_delete" src/modules --include="*.rs"

# 3. Verificar que no haya clientes HTTP reqwest sin instrumentar
grep -rn "reqwest::Client::new\|reqwest::ClientBuilder" src --include="*.rs" | grep -v "core/http_client.rs"

# 4. Verificar que no haya sintaxis de ruta errónea en Axum (:id)
grep -rn 'route("/:' src/modules --include="*.rs"

# 5. Verificar que no se use TimeoutLayer de tower-http
grep -rn "TimeoutLayer" src --include="*.rs"
```
*Salida esperada para todos los comandos: completamente vacía.*

### 3.3 Protocolo ante Fallos en la Verificación
- Si `cargo fmt --check` falla $\rightarrow$ Ejecutar `cargo fmt` y re-verificar.
- Si `cargo clippy` detecta advertencias $\rightarrow$ Corregir el código en el origen del dato. **PROHIBIDO** silenciar advertencias con `#[allow(...)]` para esconder errores.
- Si `cargo check` o `cargo test` fallan $\rightarrow$ Diagnosticar el traceback completo, corregir la discrepancia de tipos o la lógica de negocio y re-ejecutar la batería completa.
