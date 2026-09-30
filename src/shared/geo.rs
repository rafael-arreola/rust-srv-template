use std::fmt;

use serde::{Deserialize, Serialize};

use super::money::Currency;
use crate::core::error::{AppError, ErrorKind};

/// País de operación del negocio como catálogo cerrado (código ISO 3166-1 alpha-2).
///
/// Las variantes son países de ejemplo; un servicio real deja solo aquellos donde opera. La
/// representación serde es el código en mayúsculas (`"MX"`), idéntica en JSON y BSON: un documento
/// con una variante renombrada o eliminada deja de deserializar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Country {
    Mx,
    Co,
    Pe,
    Cl,
}

impl Country {
    /// Catálogo completo de países soportados, en orden estable.
    pub const SUPPORTED: [Country; 4] = [Country::Mx, Country::Co, Country::Pe, Country::Cl];

    /// Código ISO 3166-1 alpha-2.
    pub fn code(self) -> &'static str {
        match self {
            Self::Mx => "MX",
            Self::Co => "CO",
            Self::Pe => "PE",
            Self::Cl => "CL",
        }
    }

    /// Moneda de curso legal del país.
    pub fn currency(self) -> Currency {
        match self {
            Self::Mx => Currency::Mxn,
            Self::Co => Currency::Cop,
            Self::Pe => Currency::Pen,
            Self::Cl => Currency::Clp,
        }
    }
}

impl fmt::Display for Country {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl TryFrom<&str> for Country {
    type Error = AppError;

    /// Parsea un código alpha-2 con la misma regla de aceptación que `Deserialize`: coincidencia
    /// exacta en mayúsculas. Un código no soportado se rechaza con 400 `INVALID_INPUT` y un
    /// mensaje público que enumera los códigos válidos.
    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Self::SUPPORTED.into_iter().find(|country| country.code() == code).ok_or_else(|| {
            ErrorKind::Invalid {
                field: "country",
                reason: format!(
                    "unsupported country code '{code}'; expected one of {}",
                    Self::SUPPORTED.map(Country::code).join(", ")
                ),
            }
            .into()
        })
    }
}
