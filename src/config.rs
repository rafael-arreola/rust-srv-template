use dotenvy::dotenv;
use std::env;
use std::process;
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct Env {
    pub port: u16,
    pub service_env: String,
    pub service_name: String,
    pub project_id: String,
    pub mongo_url: String,
    pub mongo_db: String,
    pub redis_url: Option<String>,
    pub redis_prefix: String,
    pub debug_level: String,
    pub cors_origins: String,
    pub drain_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub msgpack_enabled: bool,
}

static CONFIG: OnceLock<Env> = OnceLock::new();

pub fn get() -> &'static Env {
    CONFIG.get_or_init(Env::load)
}

impl Env {
    fn load() -> Self {
        dotenv().ok();

        Self {
            // Variables obligatorias: sin ellas el arranque no puede continuar.
            service_name: require_env("SERVICE_NAME"),
            mongo_url: require_env("MONGO_URL"),
            mongo_db: require_env("MONGO_DB"),

            // Variables opcionales: default razonable y aviso visible en stderr.
            port: optional_env_parsed("PORT", 3000, "3000"),
            service_env: optional_env("SERVICE_ENV", "DEV"),
            project_id: optional_env("PROJECT_ID", ""),
            redis_url: read_env("REDIS_URL"),
            redis_prefix: optional_env("REDIS_PREFIX", "service"),
            debug_level: optional_env("DEBUG_LEVEL", "info"),
            cors_origins: optional_env("CORS_ORIGINS", "*"),
            drain_timeout_secs: optional_env_parsed("DRAIN_TIMEOUT_SECS", 10, "10"),
            request_timeout_secs: optional_env_parsed("REQUEST_TIMEOUT_SECS", 30, "30"),
            msgpack_enabled: optional_bool("ENABLE_MSGPACK", true),
        }
    }
}

/// Lee y normaliza una variable de entorno.
///
/// Aplica `trim`, retira comillas envolventes (`"` o `'`) y vuelve a recortar.
/// Devuelve `None` cuando la variable no existe o queda vacía tras normalizar.
fn read_env(name: &str) -> Option<String> {
    let raw = env::var(name).ok()?;
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')))
        .unwrap_or(trimmed);
    let value = unquoted.trim();
    if value.is_empty() { None } else { Some(value.to_string()) }
}

/// Variable obligatoria. Si falta o queda vacía, reporta y aborta el arranque.
fn require_env(name: &str) -> String {
    read_env(name).unwrap_or_else(|| {
        eprintln!("CRITICAL CONFIG ERROR: missing required environment variable '{name}'");
        process::exit(1);
    })
}

/// Variable obligatoria con parseo tipado. Un valor presente pero inválido es fatal.
fn require_env_parsed<T: FromStr>(name: &str) -> T {
    let value = require_env(name);
    match value.parse::<T>() {
        Ok(parsed) => parsed,
        Err(_) => {
            eprintln!(
                "CRITICAL CONFIG ERROR: environment variable '{name}' has invalid value '{value}'"
            );
            process::exit(1);
        }
    }
}

/// Variable opcional con default. Avisa en stderr cuando se usa el default.
fn optional_env(name: &str, default: &str) -> String {
    read_env(name).unwrap_or_else(|| {
        eprintln!(
            "CONFIG WARNING: environment variable '{name}' not set, using default '{default}'"
        );
        default.to_string()
    })
}

/// Variable opcional con parseo tipado. Avisa en stderr cuando se usa el default;
/// un valor presente pero inválido es fatal (lo detecta `require_env_parsed`).
fn optional_env_parsed<T: FromStr>(name: &str, default: T, default_repr: &str) -> T {
    if read_env(name).is_none() {
        eprintln!(
            "CONFIG WARNING: environment variable '{name}' not set, using default '{default_repr}'"
        );
        return default;
    }
    require_env_parsed(name)
}

/// Variable booleana opcional. Default cuando falta o está vacía; un valor no
/// reconocido es un error fatal de configuración.
fn optional_bool(name: &str, default: bool) -> bool {
    match read_env(name).as_deref() {
        None => {
            eprintln!(
                "CONFIG WARNING: environment variable '{name}' not set, using default '{default}'"
            );
            default
        }
        Some("1" | "true" | "yes" | "TRUE" | "YES") => true,
        Some("0" | "false" | "no" | "FALSE" | "NO") => false,
        Some(value) => {
            eprintln!(
                "CRITICAL CONFIG ERROR: environment variable '{name}' has invalid boolean value '{value}'"
            );
            process::exit(1);
        }
    }
}
