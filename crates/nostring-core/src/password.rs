//! Password entropy estimation and strength warnings
//!
//! Provides entropy estimation for encryption passwords used to protect
//! seed material. Uses a conservative approach based on character class
//! analysis and common password detection.
//!
//! # Entropy Levels
//!
//! | Level     | Bits   | Meaning                                    |
//! |-----------|--------|--------------------------------------------|
//! | Dangerous | < 28   | Trivially brute-forceable                  |
//! | Weak      | 28–35  | Vulnerable to targeted attack              |
//! | Fair      | 36–59  | Adequate for casual threats                |
//! | Strong    | 60–127 | Resistant to well-funded attackers          |
//! | Excellent | ≥ 128  | Beyond brute-force for foreseeable future   |
//!
//! # Important
//!
//! This is a **warning system**, not a gate. Users can still choose weak
//! passwords — sovereignty means respecting their choice while ensuring
//! they understand the risk.

use std::collections::HashSet;

/// Minimum recommended entropy for seed encryption (bits)
pub const MIN_RECOMMENDED_ENTROPY: f64 = 60.0;

/// Password strength level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PasswordStrength {
    /// < 28 bits — trivially crackable
    Dangerous,
    /// 28–35 bits — vulnerable to targeted attack
    Weak,
    /// 36–59 bits — adequate for casual threats
    Fair,
    /// 60–127 bits — resistant to well-funded attackers
    Strong,
    /// ≥ 128 bits — beyond brute-force
    Excellent,
}

impl PasswordStrength {
    /// Human-readable description of the strength level
    pub fn description(&self) -> &'static str {
        match self {
            Self::Dangerous => "Dangerous — trivially crackable, do not use for seed encryption",
            Self::Weak => "Weak — vulnerable to targeted attacks",
            Self::Fair => {
                "Fair — adequate for casual threats but not recommended for seed encryption"
            }
            Self::Strong => "Strong — resistant to well-funded attackers",
            Self::Excellent => "Excellent — beyond brute-force for the foreseeable future",
        }
    }

    /// Whether this strength level meets the minimum recommendation
    pub fn is_recommended(&self) -> bool {
        *self >= Self::Strong
    }
}

/// Result of password entropy analysis
#[derive(Debug, Clone)]
pub struct PasswordAnalysis {
    /// Estimated entropy in bits
    pub entropy_bits: f64,
    /// Strength classification
    pub strength: PasswordStrength,
    /// Specific warnings (empty if no issues)
    pub warnings: Vec<String>,
    /// Whether the password meets minimum recommendations
    pub meets_minimum: bool,
}

/// Common weak passwords and patterns to detect
const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "123456",
    "12345678",
    "qwerty",
    "abc123",
    "monkey",
    "1234567",
    "letmein",
    "trustno1",
    "dragon",
    "baseball",
    "iloveyou",
    "master",
    "sunshine",
    "ashley",
    "bailey",
    "shadow",
    "123456789",
    "1234567890",
    "password1",
    "bitcoin",
    "satoshi",
    "nakamoto",
    "hodl",
    "moon",
    "lambo",
    "seed",
    "wallet",
    "crypto",
];

/// Estimate the entropy of a password in bits.
///
/// Uses character class analysis with penalties for:
/// - Common passwords
/// - Short length
/// - Repeated characters
/// - Sequential patterns
///
/// # Example
/// ```
/// use nostring_core::password::estimate_entropy;
/// let analysis = estimate_entropy("correct horse battery staple");
/// assert!(analysis.entropy_bits > 60.0);
/// assert!(analysis.strength.is_recommended());
/// ```
pub fn estimate_entropy(password: &str) -> PasswordAnalysis {
    let mut warnings = Vec::new();

    // Empty password
    if password.is_empty() {
        return PasswordAnalysis {
            entropy_bits: 0.0,
            strength: PasswordStrength::Dangerous,
            warnings: vec!["Password is empty".to_string()],
            meets_minimum: false,
        };
    }

    // Check against common passwords (case-insensitive)
    let lower = password.to_lowercase();
    if COMMON_PASSWORDS
        .iter()
        .any(|&cp| lower == cp || lower.contains(cp))
    {
        warnings.push("Contains a commonly used password or word".to_string());
    }

    // Character class analysis
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;
    let mut has_unicode = false;

    for ch in password.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else if ch.is_ascii_punctuation() || ch == ' ' {
            has_symbol = true;
        } else {
            has_unicode = true;
        }
    }

    // Calculate character space size
    let mut charset_size: f64 = 0.0;
    if has_lower {
        charset_size += 26.0;
    }
    if has_upper {
        charset_size += 26.0;
    }
    if has_digit {
        charset_size += 10.0;
    }
    if has_symbol {
        charset_size += 33.0;
    }
    if has_unicode {
        charset_size += 100.0; // conservative estimate for common Unicode
    }

    // Ensure minimum charset
    if charset_size < 1.0 {
        charset_size = 1.0;
    }

    let len = password.chars().count() as f64;

    // Base entropy: log2(charset_size) * length
    let mut entropy = len * charset_size.log2();

    // Penalty: repeated characters reduce effective entropy
    let unique_chars: HashSet<char> = password.chars().collect();
    let unique_ratio = unique_chars.len() as f64 / len;
    if unique_ratio < 0.5 {
        let penalty = (1.0 - unique_ratio) * entropy * 0.3;
        entropy -= penalty;
        warnings.push("Too many repeated characters".to_string());
    }

    // Penalty: sequential patterns (abc, 123, qwerty)
    let sequential_count = count_sequential(password);
    if sequential_count > 2 {
        let penalty = sequential_count as f64 * 2.0;
        entropy -= penalty;
        warnings.push("Contains sequential patterns".to_string());
    }

    // Penalty: all same case with no digits/symbols
    if (has_lower != has_upper) && !has_digit && !has_symbol {
        entropy *= 0.85;
        if password.len() < 12 {
            warnings
                .push("Single character class — add numbers, symbols, or mixed case".to_string());
        }
    }

    // Bonus: passphrase detection (4+ words separated by spaces)
    let word_count = password.split_whitespace().count();
    if word_count >= 4 {
        // Passphrases get a slight bonus — word-based entropy is higher
        // than character-based for the same length
        let word_bonus = (word_count as f64 - 3.0) * 3.0;
        entropy += word_bonus;
    }

    // Floor at 0
    if entropy < 0.0 {
        entropy = 0.0;
    }

    // Length warning
    if password.len() < 8 {
        warnings.push("Password is very short (< 8 characters)".to_string());
    } else if password.len() < 12 {
        warnings.push("Consider a longer password (12+ characters recommended)".to_string());
    }

    // Classify
    let strength = if entropy < 28.0 {
        PasswordStrength::Dangerous
    } else if entropy < 36.0 {
        PasswordStrength::Weak
    } else if entropy < 60.0 {
        PasswordStrength::Fair
    } else if entropy < 128.0 {
        PasswordStrength::Strong
    } else {
        PasswordStrength::Excellent
    };

    PasswordAnalysis {
        entropy_bits: entropy,
        strength,
        warnings,
        meets_minimum: strength >= PasswordStrength::Strong,
    }
}

/// Count sequential character patterns (abc, 123, etc.)
fn count_sequential(password: &str) -> usize {
    let chars: Vec<u32> = password.chars().map(|c| c as u32).collect();
    let mut count = 0;

    for window in chars.windows(3) {
        let (a, b, c) = (window[0], window[1], window[2]);
        // Ascending: a, a+1, a+2
        if b == a + 1 && c == b + 1 {
            count += 1;
        }
        // Descending: a, a-1, a-2
        if a > 1 && b == a - 1 && c == b - 1 {
            count += 1;
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_password() {
        let analysis = estimate_entropy("");
        assert_eq!(analysis.entropy_bits, 0.0);
        assert_eq!(analysis.strength, PasswordStrength::Dangerous);
        assert!(!analysis.meets_minimum);
        assert!(!analysis.warnings.is_empty());
    }

    #[test]
    fn test_common_password_detected() {
        let analysis = estimate_entropy("password");
        assert!(analysis
            .warnings
            .iter()
            .any(|w| w.contains("commonly used")));
        assert!(!analysis.meets_minimum);

        let analysis = estimate_entropy("bitcoin");
        assert!(analysis
            .warnings
            .iter()
            .any(|w| w.contains("commonly used")));

        let analysis = estimate_entropy("satoshi");
        assert!(analysis
            .warnings
            .iter()
            .any(|w| w.contains("commonly used")));
    }

    #[test]
    fn test_short_password_warned() {
        let analysis = estimate_entropy("abc");
        assert!(analysis.warnings.iter().any(|w| w.contains("very short")));
        assert_eq!(analysis.strength, PasswordStrength::Dangerous);
    }

    #[test]
    fn test_digits_only_weak() {
        // 6-digit PIN
        let analysis = estimate_entropy("123456");
        assert!(analysis.strength <= PasswordStrength::Weak);

        // 8-digit PIN
        let analysis = estimate_entropy("12345678");
        assert!(analysis.strength <= PasswordStrength::Weak);
    }

    #[test]
    fn test_passphrase_strong() {
        let analysis = estimate_entropy("correct horse battery staple");
        assert!(
            analysis.strength >= PasswordStrength::Strong,
            "classic passphrase should be Strong, got {:?} ({:.1} bits)",
            analysis.strength,
            analysis.entropy_bits
        );
        assert!(analysis.meets_minimum);
    }

    #[test]
    fn test_long_mixed_excellent() {
        let analysis = estimate_entropy("Tr0ub4dor&3-correct-HORSE-battery!");
        assert!(
            analysis.strength >= PasswordStrength::Strong,
            "long mixed password should be Strong+, got {:?} ({:.1} bits)",
            analysis.strength,
            analysis.entropy_bits
        );
    }

    #[test]
    fn test_repeated_chars_penalized() {
        let analysis = estimate_entropy("aaaaaaaaaa");
        assert!(analysis.warnings.iter().any(|w| w.contains("repeated")));
        // Should be much weaker than 10 unique lowercase chars (non-sequential)
        let unique_analysis = estimate_entropy("qxmtpjwrkz");
        assert!(
            analysis.entropy_bits < unique_analysis.entropy_bits,
            "repeated chars ({:.1}) should have less entropy than unique ({:.1})",
            analysis.entropy_bits,
            unique_analysis.entropy_bits
        );
    }

    #[test]
    fn test_sequential_patterns_penalized() {
        let analysis = estimate_entropy("abcdefgh");
        assert!(analysis.warnings.iter().any(|w| w.contains("sequential")));

        let analysis = estimate_entropy("987654321");
        assert!(analysis.warnings.iter().any(|w| w.contains("sequential")));
    }

    #[test]
    fn test_strength_ordering() {
        assert!(PasswordStrength::Dangerous < PasswordStrength::Weak);
        assert!(PasswordStrength::Weak < PasswordStrength::Fair);
        assert!(PasswordStrength::Fair < PasswordStrength::Strong);
        assert!(PasswordStrength::Strong < PasswordStrength::Excellent);
    }

    #[test]
    fn test_is_recommended_threshold() {
        assert!(!PasswordStrength::Dangerous.is_recommended());
        assert!(!PasswordStrength::Weak.is_recommended());
        assert!(!PasswordStrength::Fair.is_recommended());
        assert!(PasswordStrength::Strong.is_recommended());
        assert!(PasswordStrength::Excellent.is_recommended());
    }

    #[test]
    fn test_unicode_password() {
        let analysis = estimate_entropy("密码是很安全的東西!");
        assert!(
            analysis.entropy_bits > 40.0,
            "Unicode password should have decent entropy, got {:.1}",
            analysis.entropy_bits
        );
    }

    #[test]
    fn test_mixed_charset_bonus() {
        // Same length, more character classes = more entropy
        let lower_only = estimate_entropy("abcdefghijkl");
        let mixed = estimate_entropy("aBcD3fGh!jKl");
        assert!(
            mixed.entropy_bits > lower_only.entropy_bits,
            "mixed charset ({:.1}) should beat lowercase only ({:.1})",
            mixed.entropy_bits,
            lower_only.entropy_bits
        );
    }

    #[test]
    fn test_realistic_passwords() {
        // Realistic weak passwords people actually use
        assert!(estimate_entropy("letmein").strength <= PasswordStrength::Weak);
        assert!(estimate_entropy("P@ssw0rd").strength <= PasswordStrength::Fair);
        assert!(estimate_entropy("iloveyou").strength <= PasswordStrength::Weak);

        // Realistic strong passwords
        assert!(estimate_entropy("purple-monkey-dishwasher-42").meets_minimum);
        assert!(estimate_entropy("The quick brown fox! 2024").meets_minimum);
    }

    #[test]
    fn test_entropy_increases_with_length() {
        let short = estimate_entropy("aB3!");
        let medium = estimate_entropy("aB3!xY7@");
        let long = estimate_entropy("aB3!xY7@mN2#pQ5&");

        assert!(
            short.entropy_bits < medium.entropy_bits,
            "short ({:.1}) should < medium ({:.1})",
            short.entropy_bits,
            medium.entropy_bits
        );
        assert!(
            medium.entropy_bits < long.entropy_bits,
            "medium ({:.1}) should < long ({:.1})",
            medium.entropy_bits,
            long.entropy_bits
        );
    }

    #[test]
    fn test_description_not_empty() {
        for strength in [
            PasswordStrength::Dangerous,
            PasswordStrength::Weak,
            PasswordStrength::Fair,
            PasswordStrength::Strong,
            PasswordStrength::Excellent,
        ] {
            assert!(!strength.description().is_empty());
        }
    }
}
