use std::fmt;

use serde::{Deserialize, Serialize};

use engine::{AppError, AppResult};

/// Moneda de curso legal (ISO 4217).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Mxn,
    Cop,
    Pen,
    Clp,
}

impl Currency {
    pub fn code(self) -> &'static str {
        match self {
            Self::Mxn => "MXN",
            Self::Cop => "COP",
            Self::Pen => "PEN",
            Self::Clp => "CLP",
        }
    }

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

/// Importe monetario en unidades menores con aritmética comprobada.
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

    pub fn amount(self) -> i64 {
        self.amount
    }

    pub fn currency(self) -> Currency {
        self.currency
    }

    pub fn is_positive(self) -> bool {
        self.amount > 0
    }

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

    pub fn checked_mul(self, factor: u32) -> AppResult<Money> {
        let amount = self
            .amount
            .checked_mul(i64::from(factor))
            .ok_or_else(|| AppError::business_rule("monetary amount overflow"))?;
        Ok(Money { amount, currency: self.currency })
    }

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

/// País de operación del negocio (ISO 3166-1 alpha-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Country {
    Mx,
    Co,
    Pe,
    Cl,
}

impl Country {
    pub const SUPPORTED: [Country; 4] = [Country::Mx, Country::Co, Country::Pe, Country::Cl];

    pub fn code(self) -> &'static str {
        match self {
            Self::Mx => "MX",
            Self::Co => "CO",
            Self::Pe => "PE",
            Self::Cl => "CL",
        }
    }

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

    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Self::SUPPORTED.into_iter().find(|country| country.code() == code).ok_or_else(|| {
            AppError::invalid(
                "country",
                format!(
                    "unsupported country code '{code}'; expected one of {}",
                    Self::SUPPORTED.map(Country::code).join(", ")
                ),
            )
        })
    }
}

const GRAMS_PER_KILOGRAM: f64 = 1000.0;

/// Magnitud física de peso expresada en kilogramos con resolución de 1 gramo (`0.001 kg`),
/// encapsulando el uso de `f64` dentro de un objeto de valor validado en `models/`.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kilograms(f64);

impl Kilograms {
    pub fn try_new(kilograms: f64) -> AppResult<Self> {
        if !kilograms.is_finite() || kilograms < 0.0 {
            return Err(AppError::invalid("weight", "weight must be a finite non-negative number"));
        }
        Ok(Self((kilograms * GRAMS_PER_KILOGRAM).round() / GRAMS_PER_KILOGRAM))
    }

    pub fn from_grams(grams: u32) -> Self {
        let raw = f64::from(grams) / GRAMS_PER_KILOGRAM;
        Self((raw * GRAMS_PER_KILOGRAM).round() / GRAMS_PER_KILOGRAM)
    }

    #[inline]
    pub fn value(self) -> f64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> AppResult<Self> {
        Self::try_new(self.0 + other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_and_multiplies_money_with_overflow_and_currency_checks() {
        let total = Money::new(1999, Currency::Mxn).checked_add(Money::new(1, Currency::Mxn));
        assert_eq!(total.expect("same currency"), Money::new(2000, Currency::Mxn));
        assert!(
            Money::new(100, Currency::Mxn).checked_add(Money::new(100, Currency::Clp)).is_err()
        );
        assert!(
            Money::new(i64::MAX, Currency::Mxn).checked_add(Money::new(1, Currency::Mxn)).is_err()
        );
        assert_eq!(
            Money::new(1999, Currency::Mxn).checked_mul(3).expect("mul"),
            Money::new(5997, Currency::Mxn)
        );
        assert_eq!(Money::try_sum(Vec::new()).expect("empty"), None);
    }
}
