use crate::DomainError;

/// A validated outcome price/probability in the range 0.0000–1.0000, stored as
/// an integer count of ten-thousandths (basis points of a whole).
///
/// The codebase deliberately avoids floating point for anything money-adjacent
/// (balances are `i64` minimal units); prices follow suit. A `Price` of
/// `10_000` means 1.0000, `5_000` means 0.5000, and `0` means 0.0000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(i32);

impl Price {
    /// Number of integer units that make up 1.0000.
    pub const SCALE: i32 = 10_000;

    /// The certain price, 1.0000.
    pub const ONE: Price = Price(Self::SCALE);

    /// The impossible price, 0.0000.
    pub const ZERO: Price = Price(0);

    /// Builds a price from ten-thousandths, rejecting anything outside
    /// `0..=10_000`.
    pub fn from_ten_thousandths(value: i32) -> Result<Self, DomainError> {
        if !(0..=Self::SCALE).contains(&value) {
            return Err(DomainError::Validation(format!(
                "price must be between 0 and {} ten-thousandths",
                Self::SCALE
            )));
        }
        Ok(Self(value))
    }

    /// The price as an integer count of ten-thousandths (0..=10_000).
    pub fn as_ten_thousandths(&self) -> i32 {
        self.0
    }

    /// The price as a fraction in `[0.0, 1.0]`. For presentation only — never
    /// use this in balance or payout arithmetic.
    pub fn as_fraction(&self) -> f64 {
        f64::from(self.0) / f64::from(Self::SCALE)
    }
}

impl std::fmt::Display for Price {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Four decimal places, matching the NUMERIC(6,4) presentation.
        write!(f, "{:.4}", self.as_fraction())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounds_and_rejects_outside() {
        assert_eq!(Price::from_ten_thousandths(0).unwrap(), Price::ZERO);
        assert_eq!(Price::from_ten_thousandths(10_000).unwrap(), Price::ONE);
        assert!(Price::from_ten_thousandths(-1).is_err());
        assert!(Price::from_ten_thousandths(10_001).is_err());
    }

    #[test]
    fn converts_to_fraction_and_string() {
        let half = Price::from_ten_thousandths(5_000).unwrap();
        assert_eq!(half.as_fraction(), 0.5);
        assert_eq!(half.to_string(), "0.5000");
    }
}
