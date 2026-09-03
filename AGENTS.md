# Directiva Operativa del Repositorio (`AGENTS.md`)

Este documento constituye la **fuente de verdad técnica, arquitectónica y operativa** para cualquier agente de IA o desarrollador que opere en `rust-srv-template`.

Estructurado bajo los **tres pilares fundamentales de diseño de instrucciones modernas** (evitando el *hobbling* o micromanagement paso a paso para permitir el máximo razonamiento del modelo):
1. **Definición del Sistema y Arquitectura (El "Qué"):** Objetivos de alto nivel, contratos de tipos y arquitectura de la solución.
2. **Guardrails y Límites Infranqueables:** Fronteras estrictas de contención y seguridad dentro de las cuales el agente tiene total libertad de razonamiento.
3. **Criterios de Término Verificables (Definition of Done & Self-Verification):** Protocolos y herramientas deterministas para que el propio agente auto-verifique empíricamente su trabajo antes de darlo por concluido.

---

## 0. Identidad y Filosofía de Operación
- **Mentalidad y Tono:** Opero con pensamiento crítico, máxima precisión técnica y convicción de directiva propia en primera persona. Comunicación directa, técnica y libre de complacencia o adulación.
- **Foco Exclusivo:** Analizar, revisar, sugerir, corregir, mejorar, cuestionar y planear.
- **Arquitectura Anti-Hobbling:** Toda la operación se rige bajo tres pilares fundamentales: **Definición del Sistema y Arquitectura** (El "Qué"), **Guardrails Infranqueables** (Límites de Contención) y **Criterios de Término Verificables** (Definition of Done), impulsados por un **bucle autónomo de auto-verificación empírica**.
- **Pensamiento Crítico y Disenso Constructivo:** Cuestiono activamente cualquier solicitud, premisa o diseño que introduzca antipatrones, fallos arquitectónicos o deuda técnica. Ante cualquier bifurcación de diseño o disenso, presento al menos dos alternativas estructuradas con sus respectivos trade-offs (complejidad, mantenibilidad, rendimiento y riesgo).

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
.
├── build/
│   ├── Dockerfile          # Multi-stage build con cargo-chef, LLD y distroless
│   └── cloudbuild.yaml     # CI/CD en GCP Cloud Build y deploy a Cloud Run
├── .github/workflows/
│   └── ci.yml              # Pipeline de GitHub Actions (fmt, clippy, check, test, audit)
├── .dockerignore           # Exclusión de artefactos y secretos del contexto Docker
├── Cargo.toml              # Manifiesto unificado con dependencias y lints de clippy
├── rustfmt.toml            # Reglas oficiales de formato (Rust 2024)
├── clippy.toml             # Reglas estrictas de linter
│
└── src/
    ├── main.rs             # Entrypoint: Inicializa crypto provider, telemetría y conexión base a BD
    ├── app.rs              # Composition Root: AppContext, orquesta dependencias y monta routers
    ├── config.rs           # Carga inmutable y tipada de entorno (.env vía OnceLock, fail-fast)
    │
    ├── core/               # Capacidades técnicas agnósticas agrupadas por familias
    │   ├── mod.rs          # Re-exports públicos unificados y retrocompatibles
    │   ├── db/             # Familia: Persistencia y Pools (MongoDB con helpers BSON resilientes, Redis)
    │   ├── http/           # Familia: Motor y Transporte Web (server, middleware, extractors, response, pagination, error)
    │   ├── telemetry/      # Familia: Observabilidad y Trazas (tracer GCP/local, format Cloud Logging)
    │   ├── net/            # Familia: Red Saliente Base (instrumented_client con timeouts y W3C)
    │   └── macros.rs       # as_json! para estructuración segura de logs en Cloud Logging
    │
    ├── vendors/            # Adaptadores hacia Servicios Externos (APIs de terceros, SDKs, Cloud)
    │   ├── mod.rs          # Re-exports de clientes de integración
    │   └── carrier/        # Vendor canónico (client HTTP instrumentado, DTOs externos, error mapping)
    │
    └── modules/            # MÓDULOS DE DOMINIO AUTOCONTENIDOS (Vertical Slices)
        ├── mod.rs          # Orquestación e instanciación topológica de módulos (Modules::init)
        ├── router.rs       # Ensamblado composable de rutas HTTP en funciones pequeñas
        ├── health/         # Probes /healthz y /readyz con verificación de drenado
        ├── users/          # Módulo canónico CRUD base (dto, model, repository, service, controller, mod)
        ├── orders/         # Módulo dependiente (ilustra consumo inter-módulo unidireccional de users)
        └── checkout/       # Módulo orquestador/complementario (coordina orders + users sin ciclos)
```

### 1.3 Contratos Arquitectónicos Esenciales
1. **Vertical Slices Cohesivos:** Cada dominio (`users`, `orders`, etc.) centraliza todos sus componentes en su propia carpeta: DTOs (`dto.rs`), persistencia BSON (`model.rs`), queries e índices (`repository.rs`), reglas de negocio (`service.rs`), handlers HTTP (`controller.rs`) y exportador del módulo (`mod.rs`).
2. **Aislamiento de Estado y Separación de Enrutamiento:** Cada módulo encapsula su propio estado (`Router::with_state(service)`). La instanciación y resolución topológica del grafo de dependencias reside en `modules/mod.rs` mediante `Modules::init(ctx)`. La definición jerárquica de endpoints, prefijos y sub-routers reside en `modules/router.rs` estructurada en funciones puras y composables (`users_routes`, `orders_routes`). Al montarse en el router principal, el estado se borra a `Router<()>`. No existe un `AppState` global monolítico.
3. **Inyección por Constructor:** Las dependencias se inyectan explícitamente en `new()` mediante referencias `Arc<T>`.
4. **Envelope Uniforme y Formatos Canónicos de Respuesta:**
   Toda respuesta de la API responde a uno de los siguientes 4 patrones estandarizados mediante `ApiResponse<T>`:
   - **Éxito Individual (`ApiResponse::ok(data)` o `ApiResponse::created(data)`):**
     Payload con status 200/201:
     ```json
     {
       "trace_id": "018f3a5b6c7d8e9f...",
       "data": { "id": "...", "name": "..." }
     }
     ```
   - **Éxito Colección Paginada (`ApiResponse::paginated(items, total, page, limit)` o `ApiResponse::ok(paginated)`):**
     Para endpoints de consulta de listados (`GET /api/v1/...`). Retorna el tipo `Paginated<T>` (alias `PaginatedResponse<T>`):
     ```json
     {
       "trace_id": "018f3a5b6c7d8e9f...",
       "data": {
         "data": [ { ... }, { ... } ],
         "total": 100,
         "total_pages": 5,
         "page": 1,
         "limit": 20
       }
     }
     ```
     `total_pages` se computa automáticamente con `(total).div_ceil(limit)`. Los handlers de listado reciben `Query(query): Query<PaginationQuery>` y extraen `query.page()` y `query.limit()`.
   - **Respuesta Sin Contenido (`ApiResponse::empty()`):**
     Status HTTP 204 No Content, sin cuerpo serializado.
   - **Error Controlado (`AppError`):**
     Status HTTP correspondiente (400, 404, 409, 422, 500, etc.):
     ```json
     {
       "trace_id": "018f3a5b6c7d8e9f...",
       "data": {
         "message": "User not found: 64f1a2...",
         "breadcrumbs": [
           "User not found: 64f1a2...",
           "Fallo en la verificación de cliente",
           "Fallo en el proceso de checkout"
         ]
       },
       "cause": "NOT_FOUND"
     }
     ```
     `breadcrumbs` en `data` es opcional: solo se incluye cuando se han apilado contextos públicos usando `.attach_public(...)`. Si no se apilan, `breadcrumbs` se omite y se mantiene el formato minimalista original. Se genera retornando `Err(AppError::...)`. El método `IntoResponse` formatea el envelope y oculta los detalles de infraestructura interna.
5. **Validación en Fronteras:** La validación sintáctica de peticiones se delega a `ValidatedBody<T>` en los DTOs. La validación semántica se realiza en los servicios de negocio.
6. **Observabilidad Unificada:** Todo endpoint y caso de uso debe ser trazable de extremo a extremo. El `trace_id` de OpenTelemetry generado en el middleware debe correlacionar la petición HTTP entrante, las queries de MongoDB y el envelope final de respuesta.
7. **Integraciones con Servicios Externos (Vendors):** Los clientes que se comunican con APIs de terceros, pasarelas o servicios en la nube residen en `src/vendors/<nombre>/`. Encapsulan sus propios DTOs externos y mapean fallos a `AppError::external_service`. Se inyectan en los servicios de negocio mediante `Arc<VendorClient>`. Si un vendor es estrictamente exclusivo de un solo módulo, puede encapsularse dentro de la carpeta de ese módulo.
8. **Topología de Módulos y Grafo Acíclico Dirigido (DAG):**
   Toda relación inter-módulo debe formar estrictamente un Grafo Acíclico Dirigido resuelto deterministamente en `Modules::init(&ctx)`:
   - **Módulos Canónicos Base:** Autocontenidos, no dependen de otros módulos de negocio (ej. `users`, `health`).
   - **Módulos Dependientes Unidireccionales:** Dependen estrictamente en una sola dirección de uno o varios módulos base vía inyección de `Arc<OtherService>` en su constructor (ej. `orders` consume `users`).
   - **Módulos Orquestadores / Complementarios (Composite Slices):** Se crean para encapsular y coordinar flujos de negocio que involucran dos o más módulos independientes (ej. `checkout` coordina `orders` y `users`), evitando que los módulos base se conozcan o se acoplen entre sí.

---

## Pilar 2: Guardrails y Límites Infranqueables

Estos límites son absolutos. El agente tiene plena autonomía para diseñar e implementar la mejor solución técnica siempre que **no traspase ninguno de estos límites**:

### 1. Límite de Dominio y Cohesión
- **PROHIBIDO** reintroducir capas horizontales globales (`src/domain/`, `src/application/`, `src/infrastructure/`). Toda lógica de un dominio pertenece a `src/modules/<dominio>/`.
- **PROHIBIDO** colocar lógica de negocio o entidades de dominio dentro de `src/core/`. `src/core/` es exclusivamente infraestructura técnica transversal y agnóstica de negocio.

### 2. Límite de Encapsulamiento de Persistencia y Fronteras de Tipos
- **PROHIBIDO** exportar o compartir modelos de base de datos BSON (`*Model`) entre módulos distintos. `UserModel` (con sus `ObjectId`, campos BSON y `deleted_at`) es un detalle de implementación privado de `modules/users/`.
- La comunicación entre módulos se realiza exclusivamente invocando el **Servicio** del otro módulo (`Arc<OtherService>`), el cual intercambia únicamente contratos públicos de salida (**DTOs** como `UserOutput`).

### 3. Límite de Dependencias Cíclicas y Grafo Acíclico de Módulos (DAG)
- **PROHIBIDO** crear dependencias circulares directas o indirectas entre módulos (`Módulo A` $\leftrightarrow$ `Módulo B`, o `A` $\rightarrow$ `B` $\rightarrow$ `C` $\rightarrow$ `A`).
- **PROHIBIDO** el antipatrón de inyección diferida mutable (`Arc<RwLock<Option<Arc<T>>>>`, `OnceCell` o inicializaciones en dos fases con mutación post-construcción) para ocultar o puentear ciclos de dependencias. Toda inyección debe ser inmutable y provista en el constructor `new()`.
- **Estrategias Mandatarias para Resolver Acoplamiento Inter-Módulo:**
  Ante cualquier interacción requerida entre dos dominios, se debe evaluar y aplicar estrictamente una de las siguientes tres estrategias en función de la semántica de negocio:
  1. **Módulo Orquestador / Complementario (Mediador de Caso de Uso - Estrategia Recomendada para Flujos Sincrónicos):**
     - **Cuándo aplica:** Cuando una operación de negocio coordinada requiere sincronismo y retorno inmediato en la petición HTTP involucrando dos o más dominios (ej. un flujo de checkout que valida órdenes y descuenta saldo o actualiza perfil de usuario).
     - **Implementación:** Se crea un módulo vertical independiente (`src/modules/<orquestador>/`) que recibe en su constructor `new()` los servicios concretos `Arc<AService>` y `Arc<BService>`.
     - **Aislamiento Estricto:** El orquestador **SOLO** tiene autorización para invocar métodos públicos del **Servicio** y consumir **DTOs públicos de salida**. **PROHIBIDO** acceder a repositorios, colecciones de MongoDB o modelos BSON internos de los módulos que coordina.
     - **Topología resultante:** Grafo Acíclico Dirigido (`Orquestador` $\rightarrow$ `A`, `Orquestador` $\rightarrow$ `B`). Los módulos `A` y `B` permanecen 100% desacoplados e ignoran la existencia mutua y la del orquestador.
  2. **Comunicación Asíncrona por Eventos Internos (Desacoplamiento Temporal):**
     - **Cuándo aplica:** Cuando la interacción es reactiva o un efecto colateral (notificaciones, auditoría, recálculo de estadísticas, invalidación de caché) donde el emisor no necesita el resultado inmediato ni condiciona su flujo a la respuesta del receptor.
     - **Implementación:** Canales in-memory `tokio::sync::broadcast` o `mpsc` instanciados en `AppContext` o durante el arranque. El emisor publica el evento; los módulos interesados se suscriben en su inicialización. Cero importaciones de tipos entre módulos.
  3. **Fusión de Bounded Context (Re-evaluación de Fronteras de Dominio):**
     - **Cuándo aplica:** Si dos módulos requieren llamadas bidireccionales continuas para operaciones CRUD elementales y no pueden operar independientemente. Esto denota una falsa fragmentación del dominio (code smell).
     - **Implementación:** Fusión de ambos conceptos en un único Vertical Slice cohesivo en lugar de añadir complejidad accidental con módulos intermediarios artificiales.

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
- **PROHIBIDO** fugar información interna de infraestructura o reglas privadas hacia el cliente en respuestas HTTP. Cadenas de conexión de base de datos, direcciones IP internas, errores de red, balance de cuentas, payloads crudos y stack traces deben quedar confinados en los logs internos correlacionados con el `trace_id`. El cliente solo recibe `AppError::public_message()`.
- **PROHIBIDO** el uso de `unwrap()` y `expect()` en código de aplicación (restringido por lints de clippy). Todo error debe propagarse con `?` y mapearse a `AppError`.
- **Doble Vista de Errores y Modelo Dual-Payload:**
  - `public_message()`: Es la única cadena de texto autorizada para enviarse en el payload de respuesta. Debe ser natural, amigable y censurada. Se puede sobrescribir con `.with_public(...)`.
  - `internal_detail()` (y `Display` / `to_string()`): Es exclusivamente para el log interno estructurado. Permite registrar detalles técnicos específicos y privados mediante `.with_internal(...)` o `AppError::censored(...)`.
  - **Acumulación de Trazas/Contexto (Breadcrumbs):** Conforme el error sube en la jerarquía (`?`), se pueden acumular capas de contexto forense mediante dos métodos complementarios (en `AppError` o en `Result<T, AppError>` vía `ContextExt`):
    - `.attach_context("...")`: Acumula contexto técnico privado en el log del servidor (`error.breadcrumbs`), jamás expuesto al cliente.
    - `.attach_public("...")`: Acumula contexto amigable y seguro en la respuesta HTTP (`data.breadcrumbs`), visible para el cliente.
- **Control de Emisión y Esquema Único de Logs de Error:**
  - **Modo Automático:** Por defecto, los errores se propagan con `?` y se registran una única vez en el choke-point de salida HTTP (`AppError::into_response`).
  - **Modo Explícito:** Si un error se recupera, o corre en una tarea en background (`tokio::spawn`), o se desea loguear antes de un rollback, se puede invocar `err.log_now()`. Esto emite el log estructurado inmediatamente y marca el error para evitar duplicación si más adelante alcanza la salida HTTP.
  - **Taxonomía Uniforme de Campos:** Todo error emitido (vía `log_now()` o `into_response()`) genera exactamente los mismos campos semánticos:
    - `error.code`: Código de error canónico (`NOT_FOUND`, `BUSINESS_RULE_VIOLATION`, `INTERNAL_ERROR`, etc.).
    - `error.message`: Mensaje público y censurado enviado al cliente.
    - `error.breadcrumbs`: Array JSON nativo con la traza secuencial completa (índice 0: causa raíz, índices 1..N: contextos agregados en el rebote).
    - `http.status_code`: Status HTTP numérico.

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

### 10. Límite de Integraciones Externas (Vendors)
- **PROHIBIDO** colocar clientes de APIs de terceros o SDKs externos dentro de `src/core/`. `src/core/` es infraestructura técnica universal agnóstica de negocio.
- **OBLIGATORIO** instanciar los clientes de vendors usando `core::net::instrumented_client()` o `client_with_timeout()`, garantizando timeouts obligatorios y propagación del header W3C `traceparent`.
- **OBLIGATORIO** mapear cualquier fallo de red o respuesta HTTP no exitosa del servicio externo a `AppError::external_service("VendorName", error_detail)`.

### 11. Límite de Control de Versiones (Git) y Zonas Protegidas
- **PROHIBIDO** ejecutar `git commit`, `git push`, `git merge`, `git rebase` o cambiar de rama sin instrucción puntual y explícita del usuario.
- **PROHIBIDO** exponer o manipular claves de API, tokens, credenciales o interactuar con bases de datos de producción. Todas las pruebas locales y verificaciones deben utilizar variables de entorno locales controladas.

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
grep -rn "reqwest::Client::new\|reqwest::ClientBuilder" src --include="*.rs" | grep -v "core/net/client.rs"

# 4. Verificar que no haya sintaxis de ruta errónea en Axum (:id)
grep -rn 'route("/:' src/modules --include="*.rs"

# 5. Verificar que no se use TimeoutLayer de tower-http
grep -rn "TimeoutLayer" src --include="*.rs"

# 6. Auditar importaciones inter-módulo para verificar que no haya ciclos
grep -rn "crate::modules::" src/modules --include="*.rs"
```
*Salida esperada para comandos 1 al 5: completamente vacía. Para el comando 6: grafo estrictamente acíclico y unidireccional (si el módulo X importa a Y, el módulo Y jamás debe importar a X).*

### 3.3 Protocolo ante Fallos en la Verificación
- Si `cargo fmt --check` falla $\rightarrow$ Ejecutar `cargo fmt` y re-verificar.
- Si `cargo clippy` detecta advertencias $\rightarrow$ Corregir el código en el origen del dato. **PROHIBIDO** silenciar advertencias con `#[allow(...)]` para esconder errores.
- Si `cargo check` o `cargo test` fallan $\rightarrow$ Diagnosticar el traceback completo, corregir la discrepancia de tipos o la lógica de negocio y re-ejecutar la batería completa.

---

## 4. Estándar de Comunicación y Toma de Decisiones Técnicas

- **Comunicación Técnica y Directa:** Lenguaje preciso, conciso y orientado a hechos. Cero preámbulos ceremoniales o adulación complaciente.
- **Formato Estructurado ante Decisiones de Arquitectura o Disensos:**
  Cuando se proponga una refactorización, un nuevo módulo o ante cualquier divergencia de diseño, se debe razonar y presentar el análisis estructurado:
  - **Opción A (Recomendada):** [Descripción técnica] — *Ventajas:* [...] | *Trade-offs / Riesgos:* [...]
  - **Opción B (Alternativa):** [Descripción técnica] — *Ventajas:* [...] | *Trade-offs / Riesgos:* [...]
  - **Dictamen Técnico:** Justificación arquitectónica objetiva basada en cohesión, acoplamiento y rendimiento en Rust.
- **Cierre con Evidencia Empírica:** Resumen conciso de cambios aplicados acompañado del resultado limpio de la batería de auto-verificación (código de salida 0).
