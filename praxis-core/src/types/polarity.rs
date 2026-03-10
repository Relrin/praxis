use serde::{Deserialize, Serialize};

/// Polarity of a constraint -- whether it prescribes or prohibits.
///
///   Positive: "we must use JWT", "always validate input"
///   Negative: "never use eval", "avoid global state", "do not hardcode secrets"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Positive,
    Negative,
}

/// Keyword sets that determine polarity.
/// A constraint whose trigger keyword is in NEGATIVE_TRIGGERS gets Polarity::Negative.
/// All others get Polarity::Positive.
pub const NEGATIVE_TRIGGERS: &[&str] = &[
    "cannot", "never", "avoid", "do not", "don't", "forbidden", "prohibited",
];

pub const POSITIVE_TRIGGERS: &[&str] = &[
    "must", "should", "required", "always",
];

impl Polarity {
    /// Returns the polarity as a lowercase string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
        }
    }

    /// Given the trigger keyword that classified a line as a constraint,
    /// determine polarity.
    pub fn from_trigger(trigger: &str) -> Self {
        let lower = trigger.to_lowercase();
        if NEGATIVE_TRIGGERS.iter().any(|&neg| lower.contains(neg)) {
            Self::Negative
        } else {
            Self::Positive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_is_negative() {
        assert_eq!(Polarity::from_trigger("cannot"), Polarity::Negative);
    }

    #[test]
    fn dont_is_negative() {
        assert_eq!(Polarity::from_trigger("don't"), Polarity::Negative);
    }

    #[test]
    fn must_is_positive() {
        assert_eq!(Polarity::from_trigger("must"), Polarity::Positive);
    }

    #[test]
    fn should_is_positive() {
        assert_eq!(Polarity::from_trigger("should"), Polarity::Positive);
    }

    #[test]
    fn never_case_insensitive() {
        assert_eq!(Polarity::from_trigger("NEVER"), Polarity::Negative);
    }

    #[test]
    fn as_str_values() {
        assert_eq!(Polarity::Positive.as_str(), "positive");
        assert_eq!(Polarity::Negative.as_str(), "negative");
    }

    #[test]
    fn serde_roundtrip() {
        let val = Polarity::Negative;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "\"negative\"");
        let back: Polarity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, val);
    }
}
