# Template Backend Modular en Rust

Plantilla de arquitectura backend de alto rendimiento diseñada bajo el patrón **Vertical Slice / Monolito Modular**. Organiza el código en módulos de negocio independientes, desacoplados y fácilmente escalables.

---

## ¿Cómo Funciona esta Arquitectura? (Explicación Sencilla)

Para entender cómo está diseñado este sistema sin necesidad de conocimientos técnicos profundos, imagina que el software funciona exactamente como una **empresa moderna con departamentos especializados**:

```text
               CLIENTE / APLICACIÓN EXTERNA
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
     [ Trámite Simple ]          [ Trámite Complejo ]
             │                           │
             ▼                           ▼
  ┌─────────────────────┐     ┌────────────────────────┐
  │  DEPARTAMENTO BASE  │     │  MÓDULO COORDINADOR    │
  │     (ej. Users)     │     │     (ej. Checkout)     │
  │                     │     │                        │
  │  • Ventanilla       │     │  • Ventanilla          │
  │  • Reglas internas  │     │  • Pasos coordinados   │
  │  • Archivador       │     └───────────┬────────────┘
  └─────────────────────┘                 │
                               ┌──────────┴──────────┐
                               ▼                     ▼
                    ┌──────────────────┐  ┌──────────────────┐
                    │  DEPTO. USUARIOS │  │  DEPTO. ÓRDENES  │
                    └──────────────────┘  └──────────────────┘
```

### 1. Los Departamentos Base (Módulos Base)
Son las áreas fundamentales del negocio (por ejemplo, **Usuarios** y **Órdenes**):
- Cada uno es un especialista en su tema: el área de usuarios solo sabe de perfiles y correos; el área de órdenes solo sabe de carritos y compras.
- Cada departamento tiene su propio **archivador privado** (base de datos). Nadie de afuera puede abrir sus cajones ni alterar sus expedientes.
- **Regla de oro:** Los departamentos base no dependen unos de otros. Son autónomos y autosuficientes.
- Cuando otro departamento necesita algo de ellos, lo pide por una **ventanilla estrecha** (`contract.rs`): Usuarios ofrece solo "consultar un usuario" y Órdenes solo "registrar una orden". Nadie recibe la llave de todo el archivador.

### 2. El Coordinador de Procesos (Módulos Orquestadores)
En el mundo real, muchas actividades requieren la participación de más de un departamento. Por ejemplo: **hacer una compra (Checkout)**:
- En lugar de que el área de órdenes intente meterse en el área de usuarios (lo que causaría caos, enredos de cables y confusiones), existe un **Coordinador** especializado (el módulo `checkout`).
- El cliente acude a la ventanilla del Coordinador.
- El Coordinador le pregunta formalmente al área de Usuarios: *"¿Este cliente existe y está al corriente?"*.
- Una vez verificado, el Coordinador le solicita al área de Órdenes: *"Por favor, genera esta orden para este cliente"*. Órdenes **no tiene ventanilla pública de creación**: la única forma de registrar una orden es a través del Coordinador, que ya verificó al cliente. Esa regla la impone la estructura, no la confianza.
- El Coordinador consolida la información y le entrega al cliente su recibo final.
- **El gran beneficio:** Si el día de mañana cambia la forma de cobrar o de validar, los departamentos base no sufren modificaciones. Todo queda limpio y sin dependencias cruzadas.

### 3. Flexibilidad Interna: No Todos los Módulos son Iguales
Un error frecuente en el software es obligar a que todas las carpetas tengan exactamente los mismos archivos y carpetas internas. En esta arquitectura la estructura se adapta a la necesidad:
- **Módulo con Datos Propios (Stateful):** Tiene ventanilla de atención (`controller`), reglas del área (`service`), archivador (`repository`), formato de expedientes (`entity` para las reglas y `document` para el archivo) y solicitudes (`dto`). *Ejemplo: Usuarios y Órdenes.*
- **Módulo Coordinador (Composite/Workflow):** No necesita archivador propio porque no guarda expedientes nuevos; solo necesita su ventanilla (`controller`), su guía de coordinación (`workflow`) y sus formatos (`dto`). *Ejemplo: Checkout.*
- **Módulo de Servicio Rápido (Lean):** Áreas que solo responden preguntas técnicas rápidas sin expedientes ni procesos largos. *Ejemplo: el probe `GET /healthz`, que responde 200 mientras el servicio atiende y 503 cuando está apagándose.*

---

## Casos de Uso Explicados Paso a Paso

### Caso de Uso A: Registro de un Nuevo Usuario (Ventanilla Única)
1. **Llegada:** El cliente envía sus datos (nombre y correo) a la ventanilla de Usuarios.
2. **Revisión:** El sistema revisa que el formulario esté bien llenado y no falten datos.
3. **Validación:** El correo se normaliza a minúsculas y el servicio verifica que no exista otro usuario activo con ese correo; si dos altas llegan al mismo tiempo, el índice único de la colección decide y ambas reciben la misma respuesta 409.
4. **Archivo:** Guarda el nuevo registro en su base de datos privada, con una versión que protege las ediciones concurrentes (dos ediciones simultáneas: la segunda recibe 409 y debe recargar).
5. **Respuesta:** Devuelve un mensaje de confirmación con el identificador del usuario creado.

### Caso de Uso B: Proceso de Checkout (Flujo Multi-Departamento)
1. **Llegada:** El cliente solicita realizar el checkout enviando su identificador de usuario y los productos a comprar. Cada precio viaja como dinero exacto: `{ "amount": 1999, "currency": "MXN" }` son 19.99 MXN, nunca un decimal flotante.
2. **Paso 1 (Identidad):** El Coordinador de Checkout consulta la ventanilla de lectura de Usuarios para confirmar que el cliente exista y esté activo.
   - *Si no existe:* Detiene el proceso inmediatamente y devuelve un mensaje claro: *"No fue posible validar la cuenta de usuario para el checkout"*.
3. **Paso 2 (Orden):** Si el cliente es válido, el Coordinador registra la orden por la única puerta de escritura de Órdenes. Órdenes calcula cada línea y el total con aritmética comprobada: una mezcla de monedas o un desborde es un error, no un número equivocado.
4. **Paso 3 (Consolidación):** El Coordinador devuelve el identificador de la orden, el del usuario, el total y el estado tal como quedó registrado. No copia el nombre ni el correo del usuario: esos datos los sirve Usuarios y una copia envejecería.

---

## Mapa de Carpetas del Sistema

```text
rust-srv-template/
├── Cargo.toml
├── AGENTS.md                   # Directiva técnica vinculante (reglas y verificación)
├── PROJECT.md                  # Brújula de negocio, arquetipos y catálogo de dominios
├── rust-toolchain.toml         # Versión de Rust clavada (build reproducible)
├── deny.toml                   # Política de supply-chain (cargo-deny)
├── scripts/audit.sh            # Batería ejecutable de invariantes arquitectónicos
│
└── src/
    ├── main.rs                 # Arranque, telemetría y ExitCode
    ├── app.rs                  # Composition root: AppContext y wiring
    ├── config.rs               # Variables de entorno tipadas (fail-fast)
    │
    ├── core/                   # Infraestructura (auth, context, db, http, net, telemetry, macros)
    ├── shared/                 # Vocabulario compartido (países, monedas)
    ├── vendors/                # Adaptadores a APIs externas
    └── modules/                # Módulos de negocio
        ├── health/             # Probe GET /healthz (Arquetipo Lean)
        ├── users/              # CRUD de usuarios + contract::UserLookup (Arquetipo Stateful)
        ├── orders/             # Lectura de órdenes + contract::OrderPlacement (Arquetipo Stateful)
        └── checkout/           # Orquestador multi-módulo (Arquetipo Composite)
```

---

## Autenticación (desconectada por defecto)

`core::auth` (JWT/JWKS + RBAC) viene incluida en la plantilla, pero se entrega **desconectada**: ningún módulo de `src/modules/` ni el router la utilizan todavía.
Para activarla, conéctala en el composition root (`src/app.rs`) siguiendo `AGENTS.md` Guardrail 17 y el catálogo de capacidades de `PROJECT.md` §2.3, sin acoplar los módulos de negocio a la capa técnica.

---

## Cambiar de Motor de Datos

Los módulos reciben un `Store` neutral (`core::db`) y nunca nombran el driver. Por cada módulo con datos propios, solo `repository.rs` y `document.rs` conocen MongoDB; `entity.rs`, `dto/`, `service.rs`, `contract.rs`, `controller.rs`, `app.rs` y el cableado de módulos no cambian al sustituir el motor. La lista exacta de lo que debe aportar un segundo backend está en `PROJECT.md` §2.6.

## Decisiones de Despliegue

El template no decide por ti si el servicio es público, en qué entorno corre ni cuánto loguea. Cada uno de esos valores, su default seguro y el significado de cada opción están en `PROJECT.md` §2.5; `bash scripts/audit.sh` imprime (inspección H) las decisiones que un clon todavía no tomó.

## Guía Rápida de Ejecución

### 1. Variables de Entorno
El archivo `.env.example` documenta las variables requeridas y opcionales. Las tres requeridas son `SERVICE_NAME`, `MONGO_URL` y `MONGO_DB`.
```bash
cp .env.example .env
```

### 2. Iniciar el Servidor en Desarrollo
```bash
cargo run
```

### 3. Verificar que Todo esté en Orden

Las compuertas que corre `build/Dockerfile` (si alguna falla, no hay imagen ni deploy) y qué añade la imagen están definidas en un solo lugar: `AGENTS.md` §3.1. Córrelas todas antes de dar algo por terminado.

Los tests viven junto al código (`#[cfg(test)] mod tests`). `cargo test` corre los unitarios y los HTTP en memoria; los de integración necesitan un MongoDB desechable y se corren con `cargo test -- --ignored` (comando exacto en `AGENTS.md` §3.1).

### 4. Agregar un Módulo Nuevo

Los pasos exactos (un solo archivo de cableado, `src/modules/mod.rs`) están en `AGENTS.md` §1.2; el arquetipo que te toca lo decide la guía de `PROJECT.md` §4.
