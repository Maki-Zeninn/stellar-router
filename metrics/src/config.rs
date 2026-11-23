//! Shared helpers for reading configuration from environment variables.
//!
//! Unset variables fall back to the given default silently; set but
//! unparsable values also fall back to the default, but log a warning naming
//! the variable so a typo does not go unnoticed.

use std::env;
use std::str::FromStr;
use tracing::warn;

/// Read a boolean flag. Accepts `1/true/yes/on` and `0/false/no/off`
/// (case-insensitive, surrounding whitespace ignored).
pub fn env_bool(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => parse_bool(&value).unwrap_or_else(|| {
            warn!(
                "Invalid value {:?} for {}; expected 1/true/yes/on or 0/false/no/off. Using default {}",
                value, name, default
            );
            default
        }),
        Err(_) => default,
    }
}

/// Read a value parsed with [`FromStr`].
pub fn env_parse<T>(name: &str, default: T) -> T
where
    T: FromStr + std::fmt::Display,
{
    match env::var(name) {
        Ok(value) => value.trim().parse().unwrap_or_else(|_| {
            warn!(
                "Invalid value {:?} for {}. Using default {}",
                value, name, default
            );
            default
        }),
        Err(_) => default,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_accepts_truthy_values() {
        for value in ["1", "true", "TRUE", "yes", "On", " true "] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }
    }

    #[test]
    fn parse_bool_accepts_falsy_values() {
        for value in ["0", "false", "No", "OFF"] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }
    }

    #[test]
    fn parse_bool_rejects_other_values() {
        for value in ["", "enabled", "2", "y"] {
            assert_eq!(parse_bool(value), None, "{value}");
        }
    }

    #[test]
    fn env_parse_falls_back_on_unset_and_invalid_values() {
        let name = "ROUTER_TEST_ENV_PARSE_HELPER";
        env::remove_var(name);
        assert_eq!(env_parse(name, 42u64), 42);

        env::set_var(name, "1h");
        assert_eq!(env_parse(name, 42u64), 42);

        env::set_var(name, "7");
        assert_eq!(env_parse(name, 42u64), 7);

        env::remove_var(name);
    }

    #[test]
    fn env_bool_falls_back_on_unset_and_invalid_values() {
        let name = "ROUTER_TEST_ENV_BOOL_HELPER";
        env::remove_var(name);
        assert!(!env_bool(name, false));

        env::set_var(name, "maybe");
        assert!(env_bool(name, true));

        env::set_var(name, "yes");
        assert!(env_bool(name, false));

        env::remove_var(name);
    }
}
