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
- **Brújula de Contexto y Mantenimiento de `PROJECT.md`:** Antes de formular cambios o diseñar nuevos dominios, consulto `PROJECT.md` como referente del propósito, visión y catálogo de capacidades del sistema. Al crear, eliminar o refactorizar módulos de dominio, arquetipos o vendors, mantengo actualizado `PROJECT.md` de forma concisa como parte de la entrega atómica.
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
├── PROJECT.md              # Brújula estratégica de contexto de negocio y guía para el LLM
├── AGENTS.md               # Directiva canónica de operación y guardrails técnicos
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
        ├── health/         # Arquetipo Lean: probes /healthz y /readyz (controller, mod)
        ├── users/          # Arquetipo Stateful CRUD: entidad persistente (dto, entity, document, repo, service, controller, mod)
        ├── orders/         # Arquetipo Stateful CRUD: entidad persistente (dto, entity, document, repo, service, controller, mod)
        └── checkout/       # Arquetipo Composite: orquestador multi-módulo (dto, workflow, controller, mod)
```

### 1.3 Contratos Arquitectónicos Esenciales
1. **Vertical Slices Cohesivos y Arquetipos Modulares Flexibles:**
   Un módulo centraliza todos sus componentes en su propia carpeta bajo `src/modules/<modulo>/`. **La estructura interna NO es fija de 6 archivos**, sino que se adapta estrictamente a uno de los siguientes **4 Arquetipos de Módulo**:
   - **Arquetipo 1: Stateful Domain Slice (CRUD Completo / Persistente):**
     Para entidades con colección propia en base de datos. Implementa la **Tríada Canónica de Separación**:
     - `dto.rs`: Frontera de transporte HTTP (inputs con validación sintáctica `validator` y outputs serializables).
     - `entity.rs`: Entidad de dominio pura con reglas de negocio e invariantes. Desacoplada de MongoDB, sin anotaciones BSON ni tipos de infraestructura (`id: String`).
     - `document.rs`: Esquema de almacenamiento BSON privado (`ObjectId`, `_id`, `deleted_at`). Implementa mappers bidireccionales (`from_domain`, `into_domain`).
     *Archivos:* `mod.rs`, `controller.rs` (o `routes.rs`), `service.rs`, `repository.rs`, `entity.rs`, `document.rs`, `dto.rs` (ej. `users`, `orders`).
   - **Arquetipo 2: Composite / Workflow Slice (Orquestador Multi-Dominio):**
     Para flujos de negocio o casos de uso coordinados entre dos o más módulos base.
     *Archivos:* `mod.rs`, `controller.rs`, `workflow.rs` (o `service.rs`), `dto.rs` (ej. `checkout`).
     *Regla estricta:* **PROHIBIDO** incluir `document.rs` o `repository.rs`; la persistencia se delega a través de los servicios base inyectados.
   - **Arquetipo 3: Lean / Gateway Slice (Transporte / Utilitario):**
     Para endpoints técnicos, webhooks o probes sin reglas de dominio complejas ni base de datos propia.
     *Archivos:* `mod.rs`, `controller.rs` (o `handlers.rs`) (ej. `health`).
   - **Arquetipo 4: Worker / Consumer Slice (Procesamiento Asíncrono):**
     Para tareas en background, cron jobs o listeners de canales internos (`tokio::sync::broadcast`) sin exposición HTTP.
     *Archivos:* `mod.rs`, `consumer.rs` (o `worker.rs`), `service.rs`, `dto.rs`.
   - **Guía de Nomenclatura Semántica:**
     - `controller.rs` vs `handlers.rs` / `routes.rs`: Usar `routes.rs` o `handlers.rs` cuando el módulo es ligero o estilo puramente funcional.
     - `service.rs` vs `workflow.rs` / `usecase.rs`: Usar `workflow.rs` o `usecase.rs` en módulos orquestadores/compuestos para denotar un proceso coordinado multi-paso.
     - `dto.rs` vs `contract.rs`: Usar `contract.rs` cuando define acuerdos estables de intercambio inter-módulo.
2. **Aislamiento de Estado y Delimitación Canónica de Enrutamiento (Opción A):**
   Existe una frontera estricta e inviolable entre el enrutamiento interno del módulo y el enrutamiento global del sistema:
   - **Dentro del Módulo (`src/modules/<modulo>/controller.rs`):**
     - Define exclusivamente **rutas relativas al recurso** (`/`, `/{id}`). Ignora prefijos de API o nombres de versión.
     - Asocia verbos HTTP a funciones handler privadas (`post(create_user)`, `get(list_users)`).
     - Encapsula el estado local inyectando su propio servicio (`.with_state(service)`). Al montarse en el router padre, el estado se borra limpiamente a `Router<()>`. **PROHIBIDO** crear un `AppState` global monolítico.
     - Maneja la extracción y validación web (`ValidatedBody`, `Query`, `Path`) y formatea la respuesta en el envelope `ApiResponse`.
   - **Fuera del Módulo (`src/modules/router.rs` y `src/app.rs`):**
     - Define el prefijo global y el versionado (`/api/v1`).
     - Monta y orquesta el árbol jerárquico de sub-routers mediante `.nest("/users", users_routes(&modules.users))`.
     - Aplica middlewares transversales globales (CORS, compresión gzip, request timeout, tracing W3C).
     - **PROHIBIDO** que el módulo conozca su URL absoluta o prefijo exterior; y **PROHIBIDO** definir handlers sueltos fuera del módulo o forzar un estado monolítico.
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
   - **Módulos Base (Leaf Slices):** Autocontenidos, encapsulan una única entidad persistente o capacidad atómica (`users`, `orders`, `health`). No dependen ni importan horizontalmente a otros módulos base.
   - **Módulos Compuestos / Orquestadores (Composite / Workflow Slices):** Se crean para encapsular y coordinar flujos de negocio que involucran dos o más módulos independientes (`checkout` coordina `users` y `orders`). Reciben los servicios base vía inyección `Arc<T>` en su constructor.
   - **Módulos Dependientes Unidireccionales:** Solo admisibles cuando un módulo secundario requiere una lectura directa simple de un módulo base sin justificar un orquestador completo. Jamás bidireccional.
9. **Topología Jerárquica y Submódulos Anidados (Sub-Slices):**
   Para dominios complejos donde un Aggregate Root agrupa conceptos subordinados que no justifican un módulo de primer nivel en `src/modules/`, se permite anidar **Submódulos a un máximo de 1 nivel de profundidad** (`src/modules/<padre>/<submodulo>/`). Se reconocen formalmente dos tipologías:
   - **Tipología 1: Submódulo de Estrategias / Políticas / Generadores (Domain Strategy Sub-Slice):**
     Para encapsular polimorfismo interno, motores de cálculo, generación documental o formateadores de exportación (ej. `src/modules/manifests/documents/` con estrategias como `manielec.rs`, `dips.rs` y utilidades como `sanitizer.rs`).
     *Estructura:* `mod.rs` (define el trait o enum de la estrategia y tipos resultantes), junto a los archivos de estrategia concreta. No tiene repositorio ni controlador propio; opera sobre las entidades del Aggregate Root y es consumido por el servicio del padre.
   - **Tipología 2: Sub-Recurso REST Anidado (Nested Resource Sub-Slice):**
     Para entidades hijas cuyo ciclo de vida depende exclusivamente del Aggregate Root (ej. direcciones de usuario: `src/modules/users/addresses/`).
     *Estructura:* Posee su propio sub-router relativo (`controller.rs`), `service.rs`, `dto.rs` y `entity.rs`.
     *Enrutamiento Composable:* El router global (`src/modules/router.rs`) **SOLO** monta al padre (`.nest("/users", ...)`). Es el `controller.rs` del padre quien anida el sub-router relativo (ej. `.nest("/{id}/addresses", addresses_routes)`).
   - **Regla de la Fachada Única:** El módulo padre actúa siempre como la **única fachada pública** del agregado. Si un módulo externo requiere contratos del submódulo anidado, el padre los re-exporta explícitamente en su `mod.rs` (ej. `pub use documents::{DocumentPayload, ManifestDocumentType};`).

---

## Pilar 2: Guardrails y Límites Infranqueables

Estos límites son absolutos. El agente tiene plena autonomía para diseñar e implementar la mejor solución técnica siempre que **no traspase ninguno de estos límites**:

### 1. Límite de Dominio y Cohesión
- **PROHIBIDO** reintroducir capas horizontales globales (`src/domain/`, `src/application/`, `src/infrastructure/`). Toda lógica de un dominio pertenece a `src/modules/<dominio>/`.
- **PROHIBIDO** colocar lógica de negocio o entidades de dominio dentro de `src/core/`. `src/core/` es exclusivamente infraestructura técnica transversal y agnóstica de negocio.

### 2. Límite de Encapsulamiento de Persistencia y Fronteras de Tipos (Enforced by Compiler)
- **PROHIBIDO** exportar o compartir esquemas de base de datos BSON (`*Document`) o entidades de dominio (`entity.rs`) entre módulos distintos. `UserDocument` y `User` son detalles privados de `modules/users/`.
- **OBLIGATORIO: Encapsulamiento por Compilador con `pub(super)`:** En `src/modules/<modulo>/mod.rs`, únicamente `dto` y `service` (o `workflow`) se exportan públicamente (`pub mod`). Los submódulos internos (`document`, `entity`, `repository` y `controller`) deben declararse con `pub(super) mod` para que el compilador de Rust sea el primer guardrail infranqueable que impida la importación ilegal de detalles internos (`error[E0603]: module is private`).
- **PROHIBIDO** referenciar estructuras de dominio de otros módulos dentro de entidades (ej. `Order` jamás debe contener un struct `User`). Las referencias inter-módulo se representan estrictamente mediante **Identificadores Opacos** (`user_id: String`).
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

### 4. Límite de Submódulos Anidados y Fachada del Agregado
- **PROHIBIDO** importar archivos o módulos internos de un submódulo anidado desde fuera del módulo padre (ej. `use crate::modules::manifests::documents::manielec::...` está terminantemente prohibido). Toda interacción externa debe realizarse exclusivamente a través de los métodos públicos del servicio del padre o mediante contratos públicos re-exportados en el `mod.rs` del módulo padre.
- **OBLIGATORIO: Encapsulamiento con `pub(super)` en el Padre:** Los submódulos anidados deben declararse con `pub(super) mod <submodulo>;` o visibilidad privada en el `mod.rs` del padre para que el compilador de Rust bloquee cualquier intento de importación no autorizada desde otros módulos.
- **PROHIBIDO** anidar submódulos a más de 1 nivel de profundidad (estructuras como `src/modules/<padre>/<hijo>/<nieto>/` están prohibidas). La profundidad máxima admitida es estrictamente 1 nivel (`src/modules/<padre>/<submodulo>/`).
- **PROHIBIDO** crear submódulos anidados para conceptos que tienen ciclo de vida independiente o que son consumidos directamente por múltiples dominios ajenos al padre. Dichos conceptos deben constituirse como módulos base independientes (Leaf Slices) en `src/modules/`.

### 5. Límite de Persistencia, Concurrencia y Soft-Delete (MongoDB)
- En entidades con borrado lógico (`deleted_at`):
  - **PROHIBIDO** ejecutar borrado físico (`delete_one`, `delete_many` o `find_one_and_delete`).
  - **OBLIGATORIO** filtrar `{ "deleted_at": { "$exists": false } }` en toda consulta de búsqueda, listado, conteo y actualización.
- **Creación de Índices:**
  - **PROHIBIDO** llamar a `create_indexes()` en `main.rs` o de forma desordenada. La creación idempotente de índices debe estar encapsulada privadamente en `Repository::new(&db).await`.
  - Los índices compuestos para colecciones con soft-delete deben iniciar siempre con `deleted_at: 1` (ej. `{ "deleted_at": 1, "created_at": -1 }`).
- **Concurrencia y Atomicidad:**
  - **PROHIBIDO** el antipatrón de "lectura previa y escritura posterior" para actualizaciones condicionales de estado o balance (race conditions / TOCTOU). Se deben usar operadores atómicos de MongoDB (`$set`, `$inc`, `$push`) combinados con filtros condicionales (ej. `{ "_id": oid, "stock": { "$gte": qty } }`) en un único `update_one`.

### 6. Límite de Red, Timeouts y Clientes HTTP
- **Inbound (Peticiones Entrantes):** Todo endpoint está protegido por el middleware `core::middleware::request_timeout` configurado por `REQUEST_TIMEOUT_SECS` (default 30s).
  - **PROHIBIDO** el uso de `tower_http::timeout::TimeoutLayer` porque responde con un 408 plano sin el envelope estándar del sistema.
- **Outbound (Peticiones Salientes):**
  - **PROHIBIDO** instanciar clientes directos con `reqwest::Client::new()` o `reqwest::ClientBuilder` (no tienen timeout por defecto y pueden congelar hilos asíncronos indefinidamente).
  - **OBLIGATORIO** usar `core::http_client::instrumented_client()` o `client_with_timeout()`, los cuales fuerzan 10s de timeout global, 3s de conexión TCP y propagación del header W3C `traceparent`.

### 7. Límite de Seguridad y Exposición de Errores
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

### 8. Límite de Observabilidad y Tracing
- **OBLIGATORIO** usar `#[tracing::instrument(skip_all, fields(...))]` al instrumentar funciones. **PROHIBIDO** el uso de `#[instrument]` sin `skip_all` en métodos que reciban DTOs, entidades, conexiones o dependencias para evitar desbordamiento de memoria y serialización innecesaria con `Debug`.
- **PROHIBIDO** fugar información sensible (contraseñas, tokens, datos de tarjetas, PII no anonimizada) en los campos declarados del span (`fields(...)`) o en los eventos de log.
- **PROHIBIDO** la amplificación de logs (doble/triple logging). Si una función retorna `Result<T, AppError>`, el error se propaga con `?`. El log del error se realiza **una única vez** en el choke-point de salida HTTP (`AppError::into_response`), jamás en el repositorio ni en el servicio intermedio.
- **OBLIGATORIO** usar el prefijo `%` (Display) en identificadores de búsqueda en spans (ej. `fields(%id, %email)` o `fields(user_id = %id)`).
- **OBLIGATORIO** usar la macro `crate::as_json!(&data)` al registrar payloads u objetos estructurados en eventos de log para evitar pánicos por serialización y garantizar JSON nativo en Cloud Logging.
- **OBLIGATORIO** encadenar `.instrument(tracing::Span::current())` en cualquier tarea asíncrona desacoplada (`tokio::spawn`) para garantizar la continuidad del `trace_id`.

### 9. Límite de Sintaxis y Delimitación de Routing Web
- **Sintaxis de Rutas en Axum:** Axum utiliza llaves `{id}`, **PROHIBIDO** el uso de sintaxis de dos puntos (`:id`).
- **PROHIBIDO** acoplar controladores a prefijos globales: `controller.rs` solo define rutas relativas al recurso (`/`, `/{id}`).
- **PROHIBIDO** extraer handlers de un módulo para montarlos en un router global sin encapsular su estado (`.with_state(service)`). El estado del servicio vive y se borra a `Router<()>` en el módulo.
- **PROHIBIDO** declarar prefijos globales (`/api/v1`) o nombres canónicos de recurso dentro de los módulos. La composición jerárquica de URLs pertenece exclusivamente a `src/modules/router.rs`.

### 10. Límite de Economía de Código (Cero Dogma)
- **PROHIBIDO** inventar traits dinámicos (`Arc<dyn Trait>`) o dobles de prueba sintéticos (mocks) por defecto. Usa tipos concretos `Arc<T>` a menos que exista una necesidad técnica real e ineludible de polimorfismo dinámico en runtime.
- **PROHIBIDO** crear archivos de prueba o tests sintéticos no solicitados expresamente por el usuario.

### 11. Límite de Integraciones Externas (Vendors)
- **PROHIBIDO** colocar clientes de APIs de terceros o SDKs externos dentro de `src/core/`. `src/core/` es infraestructura técnica universal agnóstica de negocio.
- **OBLIGATORIO** instanciar los clientes de vendors usando `core::net::instrumented_client()` o `client_with_timeout()`, garantizando timeouts obligatorios y propagación del header W3C `traceparent`.
- **OBLIGATORIO** mapear cualquier fallo de red o respuesta HTTP no exitosa del servicio externo a `AppError::external_service("VendorName", error_detail)`.

### 12. Límite de Control de Versiones (Git) y Zonas Protegidas
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

# 2. Verificación estricta del linter en todos los targets (CERO advertencias permitidas)
cargo clippy --all-targets -- -D warnings

# 3. Verificación de compilación de tipos en todos los targets
cargo check --all-targets

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

# 7. Verificar que no haya fugas de persistencia privada (document) entre módulos distintos
grep -rn "crate::modules::.*::document" src/modules --include="*.rs"
```
*Salida esperada para comandos 1 al 5 y 7: completamente vacía. Para el comando 6: grafo estrictamente acíclico y unidireccional (si el módulo X importa a Y, el módulo Y jamás debe importar a X).*

### 3.3 Protocolo ante Fallos en la Verificación
- Si `cargo fmt --check` falla $\rightarrow$ Ejecutar `cargo fmt` y re-verificar.
- Si `cargo clippy` detecta advertencias $\rightarrow$ Corregir el código en el origen del dato. **PROHIBIDO** silenciar advertencias con `#[allow(...)]` para esconder errores.
- Si `cargo check` o `cargo test` fallan $\rightarrow$ Diagnosticar el traceback completo, corregir la discrepancia de tipos o la lógica de negocio y re-ejecutar la batería completa.

### 3.4 Higiene de Contexto y Mantenimiento de PROJECT.md
- Si la tarea completada introdujo un nuevo módulo de dominio, vendor o alteró el catálogo de capacidades y arquetipos del sistema, verificar empíricamente que `PROJECT.md` refleje fielmente el estado actual del repositorio antes de dar por cerrada la tarea.

---

## 4. Estándar de Comunicación y Toma de Decisiones Técnicas

- **Comunicación Técnica y Directa:** Lenguaje preciso, conciso y orientado a hechos. Cero preámbulos ceremoniales o adulación complaciente.
- **Formato Estructurado ante Decisiones de Arquitectura o Disensos:**
  Cuando se proponga una refactorización, un nuevo módulo o ante cualquier divergencia de diseño, se debe razonar y presentar el análisis estructurado:
  - **Opción A (Recomendada):** [Descripción técnica] — *Ventajas:* [...] | *Trade-offs / Riesgos:* [...]
  - **Opción B (Alternativa):** [Descripción técnica] — *Ventajas:* [...] | *Trade-offs / Riesgos:* [...]
  - **Dictamen Técnico:** Justificación arquitectónica objetiva basada en cohesión, acoplamiento y rendimiento en Rust.
- **Cierre con Evidencia Empírica:** Resumen conciso de cambios aplicados acompañado del resultado limpio de la batería de auto-verificación (código de salida 0).
