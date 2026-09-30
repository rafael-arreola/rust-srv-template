/// Serializa una estructura de forma segura para eventos estructurados de tracing.
///
/// Si la serialización falla, retorna `{"error":"..."}` en lugar de provocar un pánico. El
/// mensaje del error se serializa como string JSON, así que comillas o barras dentro de él no
/// rompen el objeto.
#[macro_export]
macro_rules! as_json {
    ($val:expr) => {
        &serde_json::to_string($val).unwrap_or_else(|e| {
            let detail = serde_json::to_string(&format!("Serialization failed: {e}"))
                .unwrap_or_else(|_| "\"Serialization failed\"".to_string());
            format!("{{\"error\":{detail}}}")
        })
    };
}

#[cfg(test)]
mod tests {
    use serde::ser::Error as _;
    use serde::{Deserialize, Serialize, Serializer};

    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("bad \"quoted\" value"))
        }
    }

    #[derive(Deserialize)]
    struct Fallback {
        error: String,
    }

    #[test]
    fn serializes_a_valid_value() {
        let out = crate::as_json!(&vec![1, 2]);
        assert_eq!(out, "[1,2]");
    }

    #[test]
    fn fallback_is_valid_json_even_when_the_error_has_quotes() {
        let out = crate::as_json!(&Unserializable);
        let parsed: Fallback = serde_json::from_str(out).expect("fallback must be valid JSON");
        assert!(parsed.error.contains("bad \"quoted\" value"));
    }
}
