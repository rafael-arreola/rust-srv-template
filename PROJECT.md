# Contexto y Brújula Estratégica del Servicio (`PROJECT.md`)

Este documento es la **brújula conceptual y de dominio del servicio**. Proporciona el marco de negocio y la guía de orientación técnica que permite a los modelos de lenguaje (LLMs) y desarrolladores tomar decisiones alineadas con la visión del proyecto sin sobre-ingeniería.

<!--
  INSTRUCCIÓN PARA EL LLM / DESARROLLADOR:
  Al clonar este boilerplate para construir un servicio real, completa las secciones 1 y 2
  con el dominio específico del nuevo proyecto y mantén actualizada la tabla de dominios.
-->

---

## 1. Misión y Propósito del Servicio

- **Nombre del Servicio:** `[NOMBRE_DEL_SERVICIO]`
- **Propósito de Negocio:** `[Describe brevemente qué problema de negocio resuelve este servicio y su propuesta de valor]`
- **Clientes y Consumidores:** `[APIs públicas, frontend web/móvil, otros microservicios, eventos asíncronos o crons]`
- **Perímetro Operativo:** `[Límites del servicio: qué hace y qué NO debe hacer]`

---

## 2. Catálogo de Dominios del Servicio

| Dominio / Slice       | Arquetipo                   | Responsabilidad de Negocio                                  |
| :-------------------- | :-------------------------- | :---------------------------------------------------------- |
| `[ej. billing]`       | `[Stateful Domain Slice]`   | `[Gestión de facturas, timbrado fiscal y notas de crédito]` |
| `[ej. notifications]` | `[Worker / Consumer Slice]` | `[Envío asíncrono de correos transaccionales y webhooks]`   |

> **Nota del Boilerplate:** Los módulos de demostración (`users`, `orders`, `checkout`) incluidos en el template actúan como referencias vivas de los Arquetipos 1 y 2. Al inicializar el proyecto real, pueden renombrarse, refactorizarse o eliminarse según las necesidades del dominio.

### 2.1 Vocabulario Compartido (Shared Kernel, `src/shared/`)

| Tipo       | Archivo    | Representación serde                             | Consumidores                                    | Reglas puras                            |
| :--------- | :--------- | :----------------------------------------------- | :---------------------------------------------- | :-------------------------------------- |
| `Country`  | `geo.rs`   | Código ISO 3166-1 alpha-2 en mayúsculas (`"MX"`) | Ninguno aún (scaffold de demostración)          | `code()`, `currency()`, `TryFrom<&str>` |
| `Currency` | `money.rs` | Código ISO 4217 en mayúsculas (`"MXN"`)          | Ninguno aún; derivada por `Country::currency()` | `code()`, `minor_unit_digits()`         |

> Un tipo se registra aquí cuando cumple la regla de dos consumidores (dos módulos, o un módulo y un vendor). Si adquiere ciclo de vida, I/O o persistencia propios, se promueve a módulo y esta tabla conserva solo el identificador. Los catálogos de `Country` y `Currency` son de demostración: ajustarlos a los países y monedas reales del servicio.

### 2.2 Arquetipos de Módulo (juego de archivos por responsabilidad)

Un módulo no tiene una lista fija de seis archivos: adopta el arquetipo que corresponda a su responsabilidad.

| Arquetipo                         | Archivos                                                                                     | Ejemplo    | Regla dura                                                                                          |
| :-------------------------------- | :------------------------------------------------------------------------------------------- | :--------- | :--------------------------------------------------------------------------------------------------- |
| **1. Stateful Domain Slice**      | `mod`, `controller`, `service`, `repository`, `entity`, `document`, `dto`                    | `users`    | Aplica la tríada de separación completa.                                                            |
| **2. Composite / Workflow Slice** | `mod`, `controller`, `workflow` (o `service`), `dto`                                         | `checkout` | **PROHIBIDO** incluir `document.rs` o `repository.rs`: persiste a través de los servicios inyectados. |
| **3. Lean / Gateway Slice**       | `mod`, `controller` (o `handlers`)                                                           | `health`   | Puede sostener **un** recurso vivo como estado del router (`.with_state(db)`); más de uno exige `service.rs`. |
| **4a. Internal Consumer Slice**   | `mod`, `consumer` (o `worker`), `service`, `dto`                                              | —          | Sin superficie HTTP: se dispara por canal `broadcast`/`mpsc`.                                        |
| **4b. HTTP Push Worker Slice**    | `mod`, `controller`, `service`, `dto`                                                        | —          | Token interno, fuera del prefijo público `/api/v1`.                                                  |
| **5. Bulk / Ingestion Pipeline**  | `mod`, `controller`, `service`, `pipeline`, `repository`, `entity`, `document`, `dto`        | —          | El chunking y el encolado viven en `pipeline.rs`; `strategies/` si hay polimorfismo; `csv.rs` solo con parser propio. |

**Regla de disparador:** si llega por canal es 4a; si llega por HTTP es 4b. La mera presencia de HTTP no convierte un slice en Arquetipo 1 o 3.

**Nomenclatura:** `routes.rs`/`handlers.rs` en slices ligeros; `workflow.rs`/`usecase.rs` en orquestadores; `contract.rs` cuando el archivo define un acuerdo de intercambio entre módulos; `command.rs` para estado de orquestación interno (`pub(super)`).

### 2.3 Capacidades Opcionales Incluidas (no cableadas)

El template trae infraestructura lista que **ningún módulo consume todavía**. Es superficie `pub` deliberada, no código muerto: existe para ser cableada, y quien clone el boilerplate decide qué activa y qué borra.

| Capacidad             | Dónde                          | Cómo se activa                                                                               |
| :-------------------- | :----------------------------- | :-------------------------------------------------------------------------------------------- |
| Autenticación JWT/RBAC| `core::auth`                   | Composition root: proveedor cripto → `Arc<dyn AuthProvider>` → `auth_middleware` por prefijo → `require_permission` por ruta (`AGENTS.md` Guardrail 17). |
| Transporte MessagePack| `core::http::extractors`       | `MsgPack<T>` en el handler, o `Accept: application/vnd.msgpack` con `ENABLE_MSGPACK=true`.   |
| Redis                 | `core::db::RedisProvider`      | Declarar `REDIS_URL`; el fallo de conexión al arranque es fatal.                             |
| Adaptador de vendor   | `vendors::carrier`             | Construir en `AppContext` con el `Arc<HttpClient>` inyectado y pasar el `Arc<CarrierClient>` al módulo que lo use. |
| Helpers BSON          | `core::db::mongo::helpers`     | `ModelId` y los deserializadores flexibles de fecha, para colecciones heredadas con tipos mixtos. |

> Si al iniciar el proyecto real una capacidad no se va a usar, **bórrala**. Mantenerla sin consumidor la deja sin presión del compilador y se pudre en silencio.

### 2.4 Fuera de Alcance del Boilerplate (decisiones abiertas)

No están incluidos y son decisión consciente de cada servicio, no olvidos: contrato publicable de API (OpenAPI), idempotencia de escrituras (`Idempotency-Key`), bus de eventos internos ya instanciado (la estrategia está definida en `AGENTS.md` Guardrail 3, la primitiva no), métricas RED y límites de tasa/concurrencia. Al necesitar cualquiera de ellos, decídelo explícitamente y regístralo aquí.

---

## 3. Principios Rectores (índice, no reglamento)

Las reglas vinculantes viven en `AGENTS.md` y, cuando son verificables, en `scripts/audit.sh`. Aquí solo queda el índice: si una entra en conflicto con `AGENTS.md`, gana `AGENTS.md`.

| Principio                                                                             | Regla vinculante          |
| :------------------------------------------------------------------------------------ | :------------------------ |
| Cohesión vertical: el dominio vive en `src/modules/<dominio>/`, sin capas horizontales | Guardrail 1               |
| Tríada de separación `dto` / `entity` / `document`; `model.rs` prohibido               | Guardrail 2, §1.2         |
| Encapsulamiento forzado por el compilador (`pub(super)`), servicios intercambian DTOs  | Guardrail 2               |
| Topología acíclica e identificadores opacos entre módulos (`user_id: String`)          | Guardrail 3               |
| Despacho estático: `Arc<T>` concreto; lista cerrada de excepciones dinámicas           | Guardrail 11              |
| Observabilidad de primera clase: `trace_id` W3C extremo a extremo                      | Guardrail 9, §1.4         |
| Shared Kernel como hoja de vocabulario, regla de dos consumidores                      | Guardrail 13, §2.1        |
| Cero DTOs mutables ni estructuras efímeras con lifetimes                               | Guardrail 14              |
| Descomposición por responsabilidad al acercarse a 400 líneas de slice                  | Guardrail 15              |

---

## 4. Guía de Decisión Rápida para el LLM (Decision Guide)

Cuando se solicite implementar una nueva capacidad, aplicar este árbol de decisión:

- **¿Requiere persistencia propia en base de datos?**
  $\rightarrow$ Crear **Arquetipo 1: Stateful Domain Slice** (`dto`, `entity`, `document`, `repository`, `service`, `controller`, `mod`).
- **¿Requiere coordinar dos o más dominios existentes?**
  $\rightarrow$ **PROHIBIDO** inyectar un módulo base en otro. Crear **Arquetipo 2: Composite / Workflow Slice** que orqueste los servicios.
- **¿Es un endpoint técnico, probe o webhook ligero sin reglas complejas?**
  $\rightarrow$ Crear **Arquetipo 3: Lean / Gateway Slice** (`controller`, `mod`).
- **¿Es una tarea en background, cron job o consumidor de eventos?**
  $\rightarrow$ Distinguir por el disparador: **Arquetipo 4a: Internal Consumer Slice** (canales `broadcast`/`mpsc`, sin HTTP) o **Arquetipo 4b: HTTP Push Worker Slice** (Cloud Tasks/webhook, `controller.rs` con token interno fuera del prefijo público). Ver la tabla de arquetipos en §2.2.
- **¿Es una ingesta masiva de archivos, procesamiento en chunks o tareas encoladas?**
  $\rightarrow$ Crear **Arquetipo 5: Bulk / Ingestion Pipeline Slice** (`dto`, `pipeline`, `entity`, `document`, `repository`, `service`, `controller`, `mod`, más `strategies/` si aplica). `csv.rs` solo si el slice tiene parser propio.
- **¿Es una integración con una API externa (ej. pasarelas, SMS, carriers)?**
  $\rightarrow$ Crear un adaptador en `src/vendors/<nombre>/` que reciba el `Arc<HttpClient>` inyectado desde el composition root, con error mapping a `AppError`.
- **¿La capacidad es autenticación, RBAC o resolución del principal?**
  $\rightarrow$ Usar la familia `core::auth` (`AuthProvider`, `AuthPrincipal`, `auth_middleware`, `require_permission`, `task_auth_middleware`), que viene **desconectada** por defecto; nunca un módulo de dominio ni lógica en el `service`.
- **¿El handler necesita el principal o el país de la petición?**
  $\rightarrow$ El middleware de contexto de petición valida e inyecta; el `controller.rs` extrae con extractores tipados y pasa primitivos al `service`. El `service` jamás lee `extensions`. Ver `AGENTS.md` Guardrail 17.
- **¿Es un tipo de vocabulario (enum de catálogo cerrado o value object) que usan dos o más módulos, o un módulo y un vendor?**
  $\rightarrow$ Colocarlo en `src/shared/` agrupado por concepto (`geo.rs`, `money.rs`, `contact.rs`) y registrarlo en §2.1. Con un solo consumidor permanece en ese módulo. Con ciclo de vida o persistencia propios, es un módulo.

Las decisiones puramente técnicas ya resueltas —dónde van las rutas, cómo se agrupan las dependencias de un `new()`, qué devuelve un repositorio paginado— no se deciden aquí: están fijadas en los Guardrails 10, 14 y 15 de `AGENTS.md`.

---

## 5. Protocolo de Mantenimiento Continuo

Cada vez que un agente o desarrollador incorpore, elimine o refactorice un módulo de dominio, un vendor o un tipo del Shared Kernel en el servicio real, **DEBE actualizar sintéticamente las secciones 2 y 2.1 de este archivo** en la misma entrega atómica.

Si el cambio introduce, elimina o modifica un arquetipo, **DEBE actualizar la tabla de §2.2** en la misma entrega. Si cambia una regla técnica vinculante, el archivo a tocar es `AGENTS.md` (y `scripts/audit.sh` cuando la regla sea verificable). Este archivo es la brújula de negocio; no hay un tercer documento de arquitectura.
