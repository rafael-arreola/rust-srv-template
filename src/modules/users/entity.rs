use chrono::Utc;

use crate::core::error::{AppError, AppResult};

/// Longitud mínima del nombre una vez recortado. El DTO valida la misma cota antes de recortar;
/// la entidad la garantiza después, porque `"  a"` pasa el validador y sería un nombre de un
/// carácter una vez almacenado.
const MIN_NAME_CHARS: usize = 2;

/// Entidad de dominio pura `User`.
///
/// Desacoplada de MongoDB, BSON y serde de infraestructura. Los campos son privados, así que el
/// tipo sostiene sus invariantes: `update_profile` es el único camino que mueve `updated_at`, el
/// email siempre está normalizado y el nombre nunca queda por debajo de `MIN_NAME_CHARS`.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    id: String,
    name: String,
    email: String,
    version: u32,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl User {
    /// Usuario aún no persistido: el identificador lo asigna el repositorio al insertarlo
    /// (ver [`User::with_id`]) y nunca sale del service en este estado. `now` lo lee el service:
    /// la entidad no consulta el reloj, así que alta y última edición salen del mismo instante.
    pub fn new(name: String, email: String, now: chrono::DateTime<Utc>) -> AppResult<Self> {
        Ok(Self {
            id: String::new(),
            name: Self::normalize_name(&name)?,
            email: Self::normalize_email(&email),
            version: 0,
            created_at: now,
            updated_at: now,
        })
    }

    /// Rehidrata un usuario leído del almacenamiento, sin recalcular nada: los invariantes ya se
    /// aplicaron al escribirlo.
    pub fn restore(
        id: String,
        name: String,
        email: String,
        version: u32,
        created_at: chrono::DateTime<Utc>,
        updated_at: chrono::DateTime<Utc>,
    ) -> Self {
        Self { id, name, email, version, created_at, updated_at }
    }

    /// Identificador asignado por el almacenamiento tras la inserción.
    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }

    pub fn update_profile(
        &mut self,
        name: String,
        email: String,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<()> {
        self.name = Self::normalize_name(&name)?;
        self.email = Self::normalize_email(&email);
        self.updated_at = now;
        Ok(())
    }

    /// El nombre se recorta y no puede quedar por debajo de [`MIN_NAME_CHARS`].
    fn normalize_name(raw: &str) -> AppResult<String> {
        let trimmed = raw.trim();
        if trimmed.chars().count() < MIN_NAME_CHARS {
            return Err(AppError::invalid(
                "name",
                format!("must have at least {MIN_NAME_CHARS} characters"),
            ));
        }
        Ok(trimmed.to_string())
    }

    /// El email se compara y almacena en minúsculas: `Ana@x.com` y `ana@x.com` son la misma
    /// cuenta, y el índice único de la colección opera sobre esta forma canónica.
    fn normalize_email(raw: &str) -> String {
        raw.trim().to_ascii_lowercase()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    /// Versión para el bloqueo optimista: el repositorio la exige igual en la actualización y la
    /// incrementa al escribir.
    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn created_at(&self) -> chrono::DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> chrono::DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> chrono::DateTime<Utc> {
        chrono::DateTime::from_timestamp(seconds, 0).expect("valid timestamp")
    }

    #[test]
    fn new_user_normalizes_and_stamps_one_instant() {
        let user =
            User::new("  Ana  ".into(), " Ana@Example.COM ".into(), at(1_000)).expect("valid");
        assert_eq!((user.name(), user.email(), user.version()), ("Ana", "ana@example.com", 0));
        assert_eq!(user.created_at(), at(1_000));
        assert_eq!(user.updated_at(), at(1_000));
    }

    #[test]
    fn name_shorter_than_minimum_after_trim_is_rejected() {
        let err = User::new("  a".into(), "a@example.com".into(), at(0)).expect_err("too short");
        assert_eq!(err.code(), "INVALID_INPUT");
    }

    #[test]
    fn update_profile_moves_only_updated_at() {
        let mut user = User::new("Ana".into(), "ana@example.com".into(), at(1_000)).expect("valid");
        user.update_profile("Ana María".into(), "ANA.M@example.com".into(), at(2_000))
            .expect("valid");
        assert_eq!((user.name(), user.email()), ("Ana María", "ana.m@example.com"));
        assert_eq!((user.created_at(), user.updated_at()), (at(1_000), at(2_000)));
    }
}
