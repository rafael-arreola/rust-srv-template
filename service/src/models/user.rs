use serde::{Deserialize, Serialize};

use engine::{AppError, AppResult, Cast, ObjectId, UtcDateTime};

const MIN_NAME_CHARS: usize = 2;

pub type UserId = Cast<String, ObjectId>;

/// Modelo `User`: `id` es `String` en Rust/JSON y se castea a `ObjectId` en MongoDB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub version: u32,
    pub created_at: UtcDateTime,
    pub updated_at: UtcDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<UtcDateTime>,
}

impl User {
    pub fn new(name: String, email: String, now: UtcDateTime) -> AppResult<Self> {
        Ok(Self {
            id: Cast::generate(),
            name: Self::normalize_name(&name)?,
            email: Self::normalize_email(&email),
            version: 0,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        })
    }

    pub fn update_profile(
        &mut self,
        name: String,
        email: String,
        now: UtcDateTime,
    ) -> AppResult<()> {
        self.name = Self::normalize_name(&name)?;
        self.email = Self::normalize_email(&email);
        self.updated_at = now;
        Ok(())
    }

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

    fn normalize_email(raw: &str) -> String {
        raw.trim().to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> UtcDateTime {
        UtcDateTime::from_timestamp_secs(seconds).expect("valid timestamp")
    }

    #[test]
    fn normalizes_and_updates_profile_safely() {
        let mut user =
            User::new("  Ana  ".into(), " Ana@Example.COM ".into(), at(1_000)).expect("valid");
        assert_eq!(
            (user.name.as_str(), user.email.as_str(), user.version),
            ("Ana", "ana@example.com", 0)
        );
        assert_eq!(user.id.len(), 24);
        assert!(User::new("  a".into(), "a@example.com".into(), at(0)).is_err());

        user.update_profile("Ana María".into(), "ANA.M@example.com".into(), at(2_000))
            .expect("valid");
        assert_eq!((user.name.as_str(), user.email.as_str()), ("Ana María", "ana.m@example.com"));
        assert_eq!((user.created_at, user.updated_at), (at(1_000), at(2_000)));
    }
}
