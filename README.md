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

### 2. El Coordinador de Procesos (Módulos Orquestadores)
En el mundo real, muchas actividades requieren la participación de más de un departamento. Por ejemplo: **hacer una compra (Checkout)**:
- En lugar de que el área de órdenes intente meterse en el área de usuarios (lo que causaría caos, enredos de cables y confusiones), existe un **Coordinador** especializado (el módulo `checkout`).
- El cliente acude a la ventanilla del Coordinador.
- El Coordinador le pregunta formalmente al área de Usuarios: *"¿Este cliente existe y está al corriente?"*.
- Una vez verificado, el Coordinador le solicita al área de Órdenes: *"Por favor, genera esta orden para este cliente"*.
- El Coordinador consolida la información y le entrega al cliente su recibo final.
- **El gran beneficio:** Si el día de mañana cambia la forma de cobrar o de validar, los departamentos base no sufren modificaciones. Todo queda limpio y sin dependencias cruzadas.

### 3. Flexibilidad Interna: No Todos los Módulos son Iguales
Un error frecuente en el software es obligar a que todas las carpetas tengan exactamente los mismos archivos y carpetas internas. En esta arquitectura la estructura se adapta a la necesidad:
- **Módulo con Datos Propios (Stateful):** Tiene ventanilla de atención (`controller`), reglas del área (`service`), archivador (`repository`), formato de expedientes (`model`) y solicitudes (`dto`). *Ejemplo: Usuarios y Órdenes.*
- **Módulo Coordinador (Composite/Workflow):** No necesita archivador propio porque no guarda expedientes nuevos; solo necesita su ventanilla (`controller`), su guía de coordinación (`workflow`) y sus formatos (`dto`). *Ejemplo: Checkout.*
- **Módulo de Servicio Rápido (Lean):** Áreas que solo responden preguntas técnicas rápidas sin expedientes ni procesos largos. *Ejemplo: Health Checks (monitoreo del estado del servidor).*

---

## Casos de Uso Explicados Paso a Paso

### Caso de Uso A: Registro de un Nuevo Usuario (Ventanilla Única)
1. **Llegada:** El cliente envía sus datos (nombre y correo) a la ventanilla de Usuarios.
2. **Revisión:** El sistema revisa que el formulario esté bien llenado y no falten datos.
3. **Validación:** El servicio interno verifica que no exista ya otro usuario con ese mismo correo.
4. **Archivo:** Guarda el nuevo registro en su base de datos privada.
5. **Respuesta:** Devuelve un mensaje de confirmación con el identificador del usuario creado.

### Caso de Uso B: Proceso de Checkout (Flujo Multi-Departamento)
1. **Llegada:** El cliente solicita realizar el checkout enviando su identificador de usuario y los productos a comprar.
2. **Paso 1 (Identidad):** El Coordinador de Checkout contacta al servicio de Usuarios para confirmar que el cliente exista.
   - *Si no existe:* Detiene el proceso inmediatamente y devuelve un mensaje claro: *"No fue posible validar la cuenta de usuario para el checkout"*.
3. **Paso 2 (Orden):** Si el cliente es válido, el Coordinador solicita al servicio de Órdenes registrar los productos, calcular subtotales y generar el número de pedido.
4. **Paso 3 (Consolidación):** El Coordinador recopila los datos de ambas áreas (nombre y correo del usuario + detalle de la orden) y entrega una confirmación unificada al cliente.

---

## Mapa de Carpetas del Sistema

```text
rust-srv-template/
├── Cargo.toml                  # Configuración general del proyecto en Rust
├── AGENTS.md                   # Directiva técnica detallada para desarrolladores y agentes IA
│
└── src/
    ├── main.rs                 # Encendido y arranque del servidor
    ├── app.rs                  # Ensamblado inicial de los componentes
    ├── config.rs               # Carga de variables de entorno y puertos
    │
    ├── core/                   # Herramientas técnicas generales (base de datos, logs, seguridad)
    │
    └── modules/                # Módulos de Negocio
        ├── health/             # Monitor de salud del servicio (Arquetipo Lean)
        ├── users/              # Gestión de usuarios y cuentas (Arquetipo Base CRUD)
        ├── orders/             # Gestión de órdenes y pedidos (Arquetipo Base CRUD)
        └── checkout/           # Orquestador del proceso de compra (Arquetipo Workflow)
```

---

## Guía Rápida de Ejecución

### 1. Variables de Entorno
```bash
cp .env.example .env
```

### 2. Iniciar el Servidor en Desarrollo
```bash
cargo run
```

### 3. Verificar que Todo esté en Orden
```bash
cargo check
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
