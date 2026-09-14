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

---

## 3. Principios Rectores Inmutables (La Brújula del Boilerplate)

Todo desarrollo dentro de este servicio debe adherirse a los siguientes invariantes arquitectónicos:

1. **Cohesión Vertical (Vertical Slices):**
   Cada dominio centraliza sus componentes en `src/modules/<dominio>/`. Prohibido reintroducir capas horizontales globales (`src/domain/`, `src/controllers/`, `src/services/`).
2. **La Tríada de Separación de Tipos (Anti-`model.rs`):**
   En entidades con persistencia, nunca mezclar las tres naturalezas de un dato:
   - `dto.rs` (o directorio `dto/`): Transporte y validación sintáctica (`validator`, Axum). Prohibido contener lógica o comandos mutables. Segmentado en `input.rs`, `output.rs`, `query.rs` y anexos según necesidad.
   - `entity.rs` (o directorio `entity/`): Reglas de negocio, invariantes y Value Objects subordinados (Rust puro, sin MongoDB ni BSON). Opcionalmente segmentado en `root.rs`, `items.rs`, `value_objects.rs`.
   - `document.rs` (o directorio `document/`): Persistencia BSON privada (`ObjectId`, `_id`, `deleted_at`). Opcionalmente segmentado en `schema.rs`, `mappers.rs`, `subdocuments.rs`.
   - **PROHIBIDO:** crear un archivo comodín `model.rs` que colapse la tríada.
3. **Encapsulamiento Forzado por Compilador:**
   Los detalles internos (`document`, `entity`, `repository`, `controller`) son privados (`pub(super)`) al módulo. La comunicación entre módulos se realiza exclusivamente invocando el `Service` público e intercambiando `Output DTOs`. Los métodos públicos de servicio nunca reciben ni retornan entidades de dominio (`entity.rs`).
4. **Topología Acíclica en Niveles (DAG):**
   Cero dependencias circulares. Módulos base independientes (Nivel 0) se coordinan mediante módulos compuestos u orquestadores (Nivel 1). Las referencias inter-módulo utilizan identificadores opacos (`user_id: String`), jamás estructuras embebidas.
5. **Cero Dogma y Despacho Estático:**
   Priorizar tipos concretos `Arc<T>`. Prohibido crear traits dinámicos (`Arc<dyn Trait>`) o dobles de prueba (mocks) por defecto.
6. **Observabilidad como Ciudadano de Primer Nivel:**
   Toda petición genera y propaga un `trace_id` de OpenTelemetry que correlaciona la traza HTTP, las operaciones de base de datos y la respuesta.
7. **Shared Kernel como Hoja de Vocabulario:**
   Los tipos de negocio compartidos (`Country`, `Currency`, value objects) viven en `src/shared/` con orden de dependencias `core ← shared ← modules, vendors`. Solo tipos puros con representación serde estable; sin servicios, repositorios ni I/O. Regla de dos consumidores y política aditiva para variantes persistidas.
8. **Cero Estructuras Pasajeras ni DTOs Mutables:**
   Prohibido declarar comandos mutables en `dto.rs` con `raw_payload`, métodos de dominio o mutaciones de estado en vuelo. Prohibido definir estructuras efímeras con tiempos de vida (`struct Assembly<'a>`) en servicios o repositorios para agrupar parámetros.
9. **Descomposición de Slices Masivos:**
   Prohibido cualquier archivo de slice por encima de ~400 líneas (umbral universal), no solo los de ingesta. Se descompone por responsabilidad en subcomponentes dentro del slice (`pipeline.rs`, `command.rs`, `strategies/`, repositorios divididos, `dto/` segmentado). El parseo propio se aísla en `csv.rs` solo cuando existe parser propio.

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
  $\rightarrow$ Distinguir por el disparador: **Arquetipo 4a: Internal Consumer Slice** (canales `broadcast`/`mpsc`, sin HTTP) o **Arquetipo 4b: HTTP Push Worker Slice** (Cloud Tasks/webhook, `controller.rs` con token interno fuera del prefijo público). Ver `docs/ARCHITECTURE.md` §1.
- **¿Es una ingesta masiva de archivos, procesamiento en chunks o tareas encoladas?**
  $\rightarrow$ Crear **Arquetipo 5: Bulk / Ingestion Pipeline Slice** (`dto`, `pipeline`, `entity`, `document`, `repository`, `service`, `controller`, `mod`, más `strategies/` si aplica). `csv.rs` solo si el slice tiene parser propio.
- **¿Es una integración con una API externa (ej. pasarelas, SMS, carriers)?**
  $\rightarrow$ Crear un adaptador en `src/vendors/<nombre>/` que reciba el `Arc<HttpClient>` inyectado desde el composition root, con error mapping a `AppError`.
- **¿La capacidad es autenticación, RBAC o resolución del principal?**
  $\rightarrow$ Usar la familia `core::auth` (`AuthProvider`, `AuthPrincipal`, `auth_middleware`, `require_permission`, `task_auth_middleware`), que viene **desconectada** por defecto; nunca un módulo de dominio ni lógica en el `service`.
- **¿El handler necesita el principal o el país de la petición?**
  $\rightarrow$ El middleware de contexto de petición valida e inyecta; el `controller.rs` extrae con extractores tipados y pasa primitivos al `service`. El `service` jamás lee `extensions`. Ver `docs/ARCHITECTURE.md` §6.
- **¿Un `new()` de módulo supera ~4 argumentos de dependencia?**
  $\rightarrow$ Agruparlos en un `XModuleDeps` owned e inyectar; nunca `#[allow(clippy::too_many_arguments)]`.
- **¿Una consulta de repositorio devuelve elementos + total?**
  $\rightarrow$ Usar `QueryResult<T>` de `crate::core::db`; `Paginated<T>` es transporte HTTP y se arma fuera del repositorio.
- **¿Es un tipo de vocabulario (enum de catálogo cerrado o value object) que usan dos o más módulos, o un módulo y un vendor?**
  $\rightarrow$ Colocarlo en `src/shared/` agrupado por concepto (`geo.rs`, `money.rs`, `contact.rs`) y registrarlo en la sección 2.1. Con un solo consumidor permanece en ese módulo. Con ciclo de vida o persistencia propios, es un módulo.
- **¿Dónde van las rutas?**
  $\rightarrow$ Rutas relativas (`/`, `/{id}`) en `controller.rs`. El prefijo global (`/api/v1`) y el montaje `.nest()` en `src/modules/router.rs`.

---

## 5. Protocolo de Mantenimiento Continuo

Cada vez que un agente o desarrollador incorpore, elimine o refactorice un módulo de dominio, un vendor o un tipo del Shared Kernel en el servicio real, **DEBE actualizar sintéticamente las secciones 2 y 2.1 de este archivo** en la misma entrega atómica.

Además, si el cambio introduce, elimina o modifica un arquetipo o un layout de slice (archivos internos, prefijos de router, patrones de descomposición), **DEBE actualizar también `docs/ARCHITECTURE.md`** en la misma entrega. Este archivo es la brújula de negocio: las reglas técnicas vinculantes viven en `AGENTS.md`, no aquí.
