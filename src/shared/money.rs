use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};

/// Moneda de curso legal (código ISO 4217).
///
/// Representación serde como código en mayúsculas (`"MXN"`), idéntica en JSON y BSON: un
/// documento con una variante renombrada o eliminada deja de deserializar.
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

    /// Posiciones decimales de la unidad menor según ISO 4217. El mismo entero de [`Money`]
    /// significa magnitudes distintas en MXN (centavos) y CLP (pesos): cualquier conversión a
    /// decimal escala con este valor.
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

/// Importe monetario como entero de unidades menores acompañado de su moneda.
///
/// Serde: `{ "amount": 1999, "currency": "MXN" }` son 19.99 MXN; `{ "amount": 1999, "currency":
/// "CLP" }` son 1999 CLP. El entero y la moneda viajan juntos porque separados no significan nada.
/// Toda la aritmética es comprobada: un desborde o una mezcla de monedas es un `AppError`, nunca
/// un wrap silencioso ni un redondeo binario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    amount: i64,
    currency: Currency,
}

impl Money {
    pub fn new(amount: i64, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Self { amount: 0, currency }
    }

    /// Unidades menores (centavos en MXN, pesos en CLP).
    pub fn amount(self) -> i64 {
        self.amount
    }

    pub fn currency(self) -> Currency {
        self.currency
    }

    pub fn is_positive(self) -> bool {
        self.amount > 0
    }

    /// Suma dos importes de la misma moneda.
    pub fn checked_add(self, other: Money) -> AppResult<Money> {
        if self.currency != other.currency {
            return Err(AppError::business_rule(format!(
                "cannot add amounts in different currencies ({} and {})",
                self.currency, other.currency
            )));
        }
        let amount = self
            .amount
            .checked_add(other.amount)
            .ok_or_else(|| AppError::business_rule("monetary amount overflow"))?;
        Ok(Money { amount, currency: self.currency })
    }

    /// Multiplica el importe por una cantidad entera (precio unitario × unidades).
    pub fn checked_mul(self, factor: u32) -> AppResult<Money> {
        let amount = self
            .amount
            .checked_mul(i64::from(factor))
            .ok_or_else(|| AppError::business_rule("monetary amount overflow"))?;
        Ok(Money { amount, currency: self.currency })
    }

    /// Suma una colección de importes. Devuelve `None` para una colección vacía, porque sin
    /// elementos no hay moneda que atribuir al total.
    pub fn try_sum<I>(items: I) -> AppResult<Option<Money>>
    where
        I: IntoIterator<Item = Money>,
    {
        let mut total: Option<Money> = None;
        for item in items {
            total = Some(match total {
                Some(accumulated) => accumulated.checked_add(item)?,
                None => item,
            });
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_amounts_of_the_same_currency() {
        let total = Money::new(1999, Currency::Mxn).checked_add(Money::new(1, Currency::Mxn));
        assert_eq!(total.expect("same currency adds"), Money::new(2000, Currency::Mxn));
    }

    #[test]
    fn rejects_mixed_currencies() {
        let err = Money::new(100, Currency::Mxn)
            .checked_add(Money::new(100, Currency::Clp))
            .expect_err("MXN + CLP must fail");
        assert_eq!(err.code(), "BUSINESS_RULE_VIOLATION");
    }

    #[test]
    fn overflow_is_an_error_not_a_wrap() {
        assert!(
            Money::new(i64::MAX, Currency::Mxn).checked_add(Money::new(1, Currency::Mxn)).is_err()
        );
        assert!(Money::new(i64::MAX, Currency::Mxn).checked_mul(2).is_err());
    }

    #[test]
    fn multiplies_by_quantity() {
        let line = Money::new(1999, Currency::Mxn).checked_mul(3).expect("fits in i64");
        assert_eq!(line, Money::new(5997, Currency::Mxn));
    }

    #[test]
    fn sum_of_nothing_has_no_currency() {
        assert_eq!(Money::try_sum(Vec::new()).expect("empty sum"), None);
    }

    #[test]
    fn sum_propagates_currency_mismatch() {
        let items = [Money::new(1, Currency::Mxn), Money::new(1, Currency::Cop)];
        assert!(Money::try_sum(items).is_err());
    }

    #[test]
    fn serde_form_is_amount_and_iso_code() {
        let json = serde_json::to_string(&Money::new(1999, Currency::Mxn)).expect("serializes");
        assert_eq!(json, r#"{"amount":1999,"currency":"MXN"}"#);
    }

    #[test]
    fn minor_unit_digits_follow_iso_4217() {
        assert_eq!(Currency::Mxn.minor_unit_digits(), 2);
        assert_eq!(Currency::Clp.minor_unit_digits(), 0);
    }
}
