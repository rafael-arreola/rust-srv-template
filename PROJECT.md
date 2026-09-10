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

| Dominio / Slice | Arquetipo | Responsabilidad de Negocio |
| :--- | :--- | :--- |
| `[ej. billing]` | `[Stateful Domain Slice]` | `[Gestión de facturas, timbrado fiscal y notas de crédito]` |
| `[ej. notifications]` | `[Worker / Consumer Slice]` | `[Envío asíncrono de correos transaccionales y webhooks]` |

> **Nota del Boilerplate:** Los módulos de demostración (`users`, `orders`, `checkout`) incluidos en el template actúan como referencias vivas de los Arquetipos 1 y 2. Al inicializar el proyecto real, pueden renombrarse, refactorizarse o eliminarse según las necesidades del dominio.

---

## 3. Principios Rectores Inmutables (La Brújula del Boilerplate)

Todo desarrollo dentro de este servicio debe adherirse a los siguientes invariantes arquitectónicos:

1. **Cohesión Vertical (Vertical Slices):**
   Cada dominio centraliza sus componentes en `src/modules/<dominio>/`. Prohibido reintroducir capas horizontales globales (`src/domain/`, `src/controllers/`, `src/services/`).
2. **La Tríada de Separación de Tipos:**
   En entidades con persistencia, nunca mezclar las tres naturalezas de un dato:
   - `dto.rs`: Transporte y validación sintáctica (`validator`, Axum).
   - `entity.rs`: Reglas de negocio e invariantes puras (Rust puro, sin MongoDB ni BSON).
   - `document.rs`: Persistencia BSON privada (`ObjectId`, `_id`, `deleted_at`).
3. **Encapsulamiento Forzado por Compilador:**
   Los detalles internos (`document`, `entity`, `repository`, `controller`) son privados (`pub(super)`) al módulo. La comunicación entre módulos se realiza exclusivamente invocando el `Service` público e intercambiando `Output DTOs`.
4. **Topología Acíclica en Niveles (DAG):**
   Cero dependencias circulares. Módulos base independientes (Nivel 0) se coordinan mediante módulos compuestos u orquestadores (Nivel 1). Las referencias inter-módulo utilizan identificadores opacos (`user_id: String`), jamás estructuras embebidas.
5. **Cero Dogma y Despacho Estático:**
   Priorizar tipos concretos `Arc<T>`. Prohibido crear traits dinámicos (`Arc<dyn Trait>`) o dobles de prueba (mocks) por defecto.
6. **Observabilidad como Ciudadano de Primer Nivel:**
   Toda petición genera y propaga un `trace_id` de OpenTelemetry que correlaciona la traza HTTP, las operaciones de base de datos y la respuesta.

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
  $\rightarrow$ Crear **Arquetipo 4: Worker / Consumer Slice** (`consumer`, `service`, `mod`).
- **¿Es una integración con una API externa (ej. pasarelas, SMS, carriers)?**
  $\rightarrow$ Crear un adaptador en `src/vendors/<nombre>/` con `instrumented_client()` y error mapping.
- **¿Dónde van las rutas?**
  $\rightarrow$ Rutas relativas (`/`, `/{id}`) en `controller.rs`. El prefijo global (`/api/v1`) y el montaje `.nest()` en `src/modules/router.rs`.

---

## 5. Protocolo de Mantenimiento Continuo

Cada vez que un agente o desarrollador incorpore, elimine o refactorice un módulo de dominio o vendor en el servicio real, **DEBE actualizar sintéticamente la sección 2 de este archivo** en la misma entrega atómica.
