//! Validated scheduling cadences.

use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use serde::{Deserialize, Deserializer};

/// Smallest supported configured cadence.
pub const MIN_CADENCE: Duration = Duration::from_millis(100);

/// Finite configured duration that is safe to use for scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cadence(Duration);

impl Cadence {
    /// Builds a cadence from a whole-millisecond constant.
    ///
    /// # Panics
    ///
    /// Panics when `milliseconds` is below 100.
    #[must_use]
    pub const fn from_millis(milliseconds: u64) -> Self {
        assert!(
            milliseconds >= 100,
            "cadence must be at least 100 milliseconds"
        );
        Self(Duration::from_millis(milliseconds))
    }

    /// Returns the validated duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }

    /// Returns the cadence as fractional seconds for diagnostics and compatibility output.
    #[must_use]
    pub fn as_secs_f64(self) -> f64 {
        self.0.as_secs_f64()
    }

    fn try_from_seconds(seconds: f64) -> Result<Self, CadenceError> {
        if !seconds.is_finite() {
            return Err(CadenceError::NonFinite);
        }
        if seconds < MIN_CADENCE.as_secs_f64() {
            return Err(CadenceError::TooShort);
        }
        Duration::try_from_secs_f64(seconds)
            .map(Self)
            .map_err(|_| CadenceError::TooLarge)
    }
}

impl<'de> Deserialize<'de> for Cadence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = f64::deserialize(deserializer)?;
        Self::try_from_seconds(seconds).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CadenceError {
    NonFinite,
    TooShort,
    TooLarge,
}

impl Display for CadenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => formatter.write_str("cadence must be finite"),
            Self::TooShort => formatter.write_str("cadence must be at least 0.1 seconds"),
            Self::TooLarge => formatter.write_str("cadence is too large"),
        }
    }
}

#[cfg(test)]
mod tests;
