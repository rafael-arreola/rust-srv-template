use std::fmt;

use serde::{Deserialize, Serialize};

/// Moneda de curso legal (código ISO 4217).
///
/// Representación serde como código en mayúsculas (`"MXN"`), idéntica en JSON y BSON.
/// Política aditiva: nunca eliminar ni renombrar variantes mientras existan documentos que las
/// almacenen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Mxn,
    Cop,
    Pen,
    Clp,
}

impl Currency {
    /// Código ISO 4217.
    pub fn code(self) -> &'static str {
        match self {
            Self::Mxn => "MXN",
            Self::Cop => "COP",
            Self::Pen => "PEN",
            Self::Clp => "CLP",
        }
    }

    /// Posiciones decimales de la unidad menor según ISO 4217.
    /// Insumo para representar dinero como enteros de unidades menores sin punto flotante.
    pub fn minor_unit_digits(self) -> u32 {
        match self {
            Self::Mxn | Self::Cop | Self::Pen => 2,
            Self::Clp => 0,
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
