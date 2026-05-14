//! Scale profiles: threshold generation (linear, fibonacci) and bucketing.

use regex::Regex;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A scale profile with named thresholds.
#[derive(Debug, Clone)]
pub struct ScaleProfile {
    pub thresholds: Vec<u64>,
}

// ---------------------------------------------------------------------------
// Regex patterns
// ---------------------------------------------------------------------------

static SCALE_PROFILE_RE: OnceLock<Regex> = OnceLock::new();

fn get_scale_profile_re() -> &'static Regex {
    SCALE_PROFILE_RE.get_or_init(|| {
        Regex::new(r"^(?P<kind>linear|fibonacci|fibnacci)-(?P<max_value>[1-9][0-9]*)-plus$")
            .unwrap()
    })
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Generate Fibonacci-ish thresholds up to max_value.
pub fn fibonacci_thresholds(max_value: u64) -> Vec<u64> {
    if max_value <= 1 {
        return vec![1];
    }

    let mut thresholds = vec![1, 2];
    loop {
        let next = thresholds[thresholds.len() - 1] + thresholds[thresholds.len() - 2];
        if next > max_value {
            break;
        }
        thresholds.push(next);
    }

    if thresholds[thresholds.len() - 1] != max_value {
        thresholds.push(max_value);
    }

    thresholds
}

/// Generate linear thresholds from 1 to max_value.
pub fn linear_thresholds(max_value: u64) -> Vec<u64> {
    (1..=max_value).collect()
}

/// Scale threshold values by the configured multiplier.
pub fn scale_thresholds(thresholds: &[u64], multiplier: u32) -> Vec<u64> {
    thresholds
        .iter()
        .map(|v| std::cmp::max(1, (*v as u64) * multiplier as u64))
        .collect()
}

/// Parse a scale profile name into concrete thresholds.
pub fn parse_scale_profile(profile_name: &str, multiplier: u32) -> Result<ScaleProfile, String> {
    let re = get_scale_profile_re();
    let caps = re.captures(profile_name).ok_or_else(|| {
        format!(
            "Invalid scale profile. Use names like linear-5-plus, \
             linear-10-plus, fibonacci-8-plus, or fibonacci-21-plus."
        )
    })?;

    let kind = caps.name("kind").unwrap().as_str();
    let max_value: u64 = caps
        .name("max_value")
        .unwrap()
        .as_str()
        .parse()
        .map_err(|_| {
            format!(
                "Invalid scale profile. Use names like linear-5-plus, \
             linear-10-plus, fibonacci-8-plus, or fibonacci-21-plus."
            )
        })?;

    let base_thresholds = match kind {
        "linear" => linear_thresholds(max_value),
        "fibonacci" | "fibnacci" => fibonacci_thresholds(max_value),
        _ => return Err(format!("Unknown scale profile kind: {}", kind)),
    };

    let thresholds = scale_thresholds(&base_thresholds, multiplier);

    Ok(ScaleProfile { thresholds })
}

/// Create buckets from thresholds and return the bucket index.
/// Returns 0 if value is 0, otherwise returns 1..=thresholds.len().
pub fn bucket_for_value(thresholds: &[u64], input_value: u64) -> usize {
    if input_value == 0 {
        return 0;
    }
    for (index, &threshold) in thresholds.iter().enumerate() {
        if input_value <= threshold {
            return index + 1;
        }
    }
    thresholds.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_for_value_zero() {
        let thresholds = vec![1, 4, 9, 16, 25];
        assert_eq!(bucket_for_value(&thresholds, 0), 0);
    }

    #[test]
    fn test_bucket_for_value_first_threshold() {
        let thresholds = vec![1, 4, 9, 16, 25];
        assert_eq!(bucket_for_value(&thresholds, 1), 1);
        assert_eq!(bucket_for_value(&thresholds, 2), 2);
        assert_eq!(bucket_for_value(&thresholds, 3), 2);
        assert_eq!(bucket_for_value(&thresholds, 4), 2);
    }

    #[test]
    fn test_bucket_for_value_last_threshold() {
        let thresholds = vec![1, 4, 9, 16, 25];
        assert_eq!(bucket_for_value(&thresholds, 25), 5);
    }

    #[test]
    fn test_bucket_for_value_over_max() {
        let thresholds = vec![1, 4, 9, 16, 25];
        assert_eq!(bucket_for_value(&thresholds, 100), 5);
    }

    #[test]
    fn test_fibonacci_thresholds() {
        assert_eq!(fibonacci_thresholds(1), vec![1]);
        assert_eq!(fibonacci_thresholds(2), vec![1, 2]);
        assert_eq!(fibonacci_thresholds(3), vec![1, 2, 3]);
        assert_eq!(fibonacci_thresholds(5), vec![1, 2, 3, 5]);
        assert_eq!(fibonacci_thresholds(8), vec![1, 2, 3, 5, 8]);
        assert_eq!(fibonacci_thresholds(10), vec![1, 2, 3, 5, 8, 10]);
        assert_eq!(fibonacci_thresholds(21), vec![1, 2, 3, 5, 8, 13, 21]);
    }

    #[test]
    fn test_linear_thresholds() {
        assert_eq!(linear_thresholds(1), vec![1]);
        assert_eq!(linear_thresholds(3), vec![1, 2, 3]);
        assert_eq!(linear_thresholds(5), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_scale_thresholds() {
        let thresholds = vec![1, 2, 3, 5];
        let scaled = scale_thresholds(&thresholds, 2);
        assert_eq!(scaled, vec![2, 4, 6, 10]);

        let scaled = scale_thresholds(&thresholds, 5);
        assert_eq!(scaled, vec![5, 10, 15, 25]);
    }

    #[test]
    fn test_parse_scale_profile() {
        let profile = parse_scale_profile("linear-5-plus", 1).unwrap();
        assert_eq!(profile.thresholds, vec![1, 2, 3, 4, 5]);

        let profile = parse_scale_profile("fibonacci-8-plus", 1).unwrap();
        assert_eq!(profile.thresholds, vec![1, 2, 3, 5, 8]);

        let profile = parse_scale_profile("fibonacci-21-plus", 1).unwrap();
        assert_eq!(profile.thresholds, vec![1, 2, 3, 5, 8, 13, 21]);
    }

    #[test]
    fn test_parse_scale_profile_invalid() {
        assert!(parse_scale_profile("invalid-5-plus", 1).is_err());
        assert!(parse_scale_profile("unknown-5-plus", 1).is_err());
    }

    #[test]
    fn test_parse_scale_profile_with_multiplier() {
        let profile = parse_scale_profile("fibonacci-5-plus", 2).unwrap();
        assert_eq!(profile.thresholds, vec![2, 4, 6, 10]);
    }
}
