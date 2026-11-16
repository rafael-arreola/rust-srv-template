use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::env;
use std::process;
use std::sync::{OnceLock, PoisonError, RwLock};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("Required environment variable '{name}' is missing or empty")]
    Missing { name: &'static str },

    #[error("Environment variable '{name}' must contain valid UTF-8")]
    NotUnicode { name: &'static str },

    #[error("Environment variable '{name}' has an invalid value")]
    Invalid { name: &'static str },

    #[error("Failed to load .env configuration file")]
    EnvironmentFile,
}

impl From<ConfigError> for AppError {
    #[track_caller]
    fn from(err: ConfigError) -> Self {
        AppError::internal(err.to_string()).with_code("CONFIG_ERROR")
    }
}

/// Contrato implementado por el struct `Env` propio de cada microservicio (`service/src/envs.rs`).
pub trait FromEnv: Sized + Send + Sync + 'static {
    fn from_env() -> Result<Self, ConfigError>;
}

type EnvStore = HashMap<TypeId, &'static (dyn Any + Send + Sync)>;

static ENVS: OnceLock<RwLock<EnvStore>> = OnceLock::new();

fn store() -> &'static RwLock<EnvStore> {
    ENVS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Fachada global absoluta de variables de entorno tipadas.
///
/// - Se inicializa una vez en el arranque con la estructura del servicio: `Envs::init::<Env>()?`.
/// - Se consulta desde cualquier punto de la aplicación sin pasar argumentos: `Envs::get::<Env>()` (o `envs::get()`).
pub struct Envs;

impl Envs {
    /// Carga `.env` (si existe), construye `T::from_env()` y fija la instancia `'static` para todo el proceso.
    pub fn init<T: FromEnv>() -> Result<&'static T, ConfigError> {
        if let Some(existing) = Self::try_get::<T>() {
            return Ok(existing);
        }

        load_dotenv()?;
        let built = T::from_env()?;
        Ok(Self::init_with(built))
    }

    /// Fija manualmente una instancia de configuración (útil en pruebas sin tocar variables del SO).
    pub fn init_with<T: Send + Sync + 'static>(value: T) -> &'static T {
        let mut guard = store().write().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = guard.get(&TypeId::of::<T>()).and_then(|v| v.downcast_ref::<T>()) {
            return existing;
        }
        let leaked: &'static T = Box::leak(Box::new(value));
        guard.insert(TypeId::of::<T>(), leaked);
        leaked
    }

    /// Devuelve la configuración si ya fue inicializada.
    pub fn try_get<T: Send + Sync + 'static>() -> Option<&'static T> {
        let guard = store().read().unwrap_or_else(PoisonError::into_inner);
        guard.get(&TypeId::of::<T>()).and_then(|v| v.downcast_ref::<T>())
    }

    /// Acceso global directo (`&'static T`). Si aún no se llamó `init`, intenta cargarla; si la
    /// configuración es inválida o incompleta, aborta el proceso con código `1`.
    pub fn get<T: FromEnv>() -> &'static T {
        if let Some(existing) = Self::try_get::<T>() {
            return existing;
        }
        match Self::init::<T>() {
            Ok(loaded) => loaded,
            Err(err) => {
                eprintln!("CRITICAL CONFIG ERROR: {err}");
                process::exit(1);
            }
        }
    }
}

/// Carga el archivo `.env` local cuando existe; su ausencia en contenedores de producción es normal.
pub fn load_dotenv() -> Result<(), ConfigError> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(err) if err.not_found() => Ok(()),
        Err(_) => Err(ConfigError::EnvironmentFile),
    }
}

/// Tipo parseable desde una variable de entorno recortada y sin comillas envolventes.
pub trait EnvValue: Sized {
    fn parse_env(raw: &str) -> Option<Self>;
}

impl EnvValue for String {
    fn parse_env(raw: &str) -> Option<Self> {
        Some(raw.to_string())
    }
}

impl EnvValue for bool {
    fn parse_env(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        }
    }
}

/// Puertos, timeouts y límites de tamaño deben ser estrictamente mayores a cero.
macro_rules! positive_uint {
    ($($ty:ty),*) => {$(
        impl EnvValue for $ty {
            fn parse_env(raw: &str) -> Option<Self> {
                raw.parse::<$ty>().ok().filter(|v| *v > 0)
            }
        }
    )*};
}
positive_uint!(u16, u32, u64, usize);

macro_rules! signed_int {
    ($($ty:ty),*) => {$(
        impl EnvValue for $ty {
            fn parse_env(raw: &str) -> Option<Self> {
                raw.parse::<$ty>().ok()
            }
        }
    )*};
}
signed_int!(i32, i64);

/// Lista separada por comas sin elementos vacíos y sin duplicados, preservando el orden.
impl EnvValue for Vec<String> {
    fn parse_env(raw: &str) -> Option<Self> {
        let mut entries = Vec::new();
        for item in raw.split(',').map(str::trim) {
            if item.is_empty() {
                return None;
            }
            if !entries.iter().any(|existing| existing == item) {
                entries.push(item.to_string());
            }
        }
        Some(entries)
    }
}

fn clean_raw(raw: &str) -> &str {
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(trimmed);
    unquoted.trim()
}

/// Lee una variable opcional: ausente o vacía devuelve `Ok(None)`; presente pero inválida devuelve `Err`.
pub fn optional<T: EnvValue>(name: &'static str) -> Result<Option<T>, ConfigError> {
    let raw = match env::var(name) {
        Ok(val) => val,
        Err(env::VarError::NotPresent) => return Ok(None),
        Err(env::VarError::NotUnicode(_)) => return Err(ConfigError::NotUnicode { name }),
    };

    match clean_raw(&raw) {
        "" => Ok(None),
        cleaned => T::parse_env(cleaned).map(Some).ok_or(ConfigError::Invalid { name }),
    }
}

/// Exige que la variable exista, no esté vacía y sea válida para `T`.
pub fn required<T: EnvValue>(name: &'static str) -> Result<T, ConfigError> {
    optional(name)?.ok_or(ConfigError::Missing { name })
}

/// Usa `default` si la variable está ausente o vacía; falla si está presente con formato inválido.
pub fn or<T: EnvValue>(name: &'static str, default: T) -> Result<T, ConfigError> {
    Ok(optional(name)?.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct CustomServiceEnv {
        name: String,
        port: u16,
    }

    impl FromEnv for CustomServiceEnv {
        fn from_env() -> Result<Self, ConfigError> {
            Ok(Self { name: "orders-srv".into(), port: 4000 })
        }
    }

    #[test]
    fn parses_values_and_exposes_typed_static_singleton() {
        assert_eq!(u16::parse_env("8080"), Some(8080));
        assert_eq!(u16::parse_env("0"), None);
        assert_eq!(bool::parse_env("TRUE"), Some(true));
        assert_eq!(bool::parse_env("no"), Some(false));
        assert_eq!(bool::parse_env("maybe"), None);
        assert_eq!(
            Vec::<String>::parse_env("a, b, a, c"),
            Some(vec!["a".into(), "b".into(), "c".into()])
        );
        assert_eq!(Vec::<String>::parse_env("a,,b"), None);
        assert_eq!(clean_raw("  \"secret_val\"  "), "secret_val");

        let initialized = Envs::init::<CustomServiceEnv>().expect("init");
        assert_eq!(initialized.port, 4000);
        assert_eq!(Envs::get::<CustomServiceEnv>(), initialized);
    }
}
