//! RS1024: Reed-Solomon checksum for SLIP-39
//!
//! Implements the RS1024 checksum from the SLIP-39 specification.
//! This is a Reed-Solomon code over GF(1024) that guarantees detection
//! of any error affecting at most 3 words and has less than 1 in 10^9
//! chance of failing to detect more errors.
//!
//! Reference: https://github.com/satoshilabs/slips/blob/master/slip-0039.md

/// Generator polynomial coefficients for RS1024
const GEN: [u32; 10] = [
    0xe0e040, 0x1c1c080, 0x3838100, 0x7070200, 0xe0e0009, 0x1c0c2412, 0x38086c24, 0x3090fc48,
    0x21b1f890, 0x3f3f120,
];

/// Customization string for standard SLIP-39 (ext=0)
pub const CS_SHAMIR: &str = "shamir";

/// Customization string for extendable SLIP-39 (ext=1)
pub const CS_SHAMIR_EXTENDABLE: &str = "shamir_extendable";

/// Compute the RS1024 polymod over a sequence of 10-bit values
///
/// This implements the polynomial modular reduction used by the
/// Reed-Solomon checksum.
fn rs1024_polymod(values: &[u16]) -> u32 {
    let mut chk: u32 = 1;

    for &v in values {
        let b = chk >> 20;
        chk = ((chk & 0xfffff) << 10) ^ (v as u32);
        for i in 0..10 {
            if (b >> i) & 1 != 0 {
                chk ^= GEN[i];
            }
        }
    }

    chk
}

/// Verify an RS1024 checksum
///
/// # Arguments
/// * `cs` - Customization string ("shamir" or "shamir_extendable")
/// * `data` - The data including the 3-word checksum at the end (as 10-bit values)
///
/// # Returns
/// `true` if the checksum is valid
pub fn rs1024_verify_checksum(cs: &str, data: &[u16]) -> bool {
    let mut values: Vec<u16> = cs.bytes().map(|b| b as u16).collect();
    values.extend_from_slice(data);
    rs1024_polymod(&values) == 1
}

/// Create an RS1024 checksum
///
/// # Arguments
/// * `cs` - Customization string ("shamir" or "shamir_extendable")
/// * `data` - The data to checksum (without the checksum)
///
/// # Returns
/// Three 10-bit checksum values
pub fn rs1024_create_checksum(cs: &str, data: &[u16]) -> [u16; 3] {
    let mut values: Vec<u16> = cs.bytes().map(|b| b as u16).collect();
    values.extend_from_slice(data);
    values.extend_from_slice(&[0, 0, 0]);

    let polymod = rs1024_polymod(&values) ^ 1;

    [
        ((polymod >> 20) & 0x3ff) as u16,
        ((polymod >> 10) & 0x3ff) as u16,
        (polymod & 0x3ff) as u16,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polymod_identity() {
        // Empty input should return 1
        assert_eq!(rs1024_polymod(&[]), 1);
    }

    #[test]
    fn test_create_and_verify() {
        // Create some test data (10-bit values)
        let data: Vec<u16> = vec![100, 200, 300, 400, 500, 600];

        // Create checksum
        let checksum = rs1024_create_checksum(CS_SHAMIR, &data);

        // Verify: append checksum to data
        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        assert!(rs1024_verify_checksum(CS_SHAMIR, &full_data));
    }

    #[test]
    fn test_wrong_checksum_fails() {
        let data: Vec<u16> = vec![100, 200, 300, 400, 500, 600];
        let mut checksum = rs1024_create_checksum(CS_SHAMIR, &data);

        // Corrupt the checksum
        checksum[0] ^= 1;

        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        assert!(!rs1024_verify_checksum(CS_SHAMIR, &full_data));
    }

    #[test]
    fn test_single_bit_error_detected() {
        let data: Vec<u16> = vec![100, 200, 300, 400, 500, 600];
        let checksum = rs1024_create_checksum(CS_SHAMIR, &data);

        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        // Flip a bit in the data
        full_data[2] ^= 1;

        assert!(!rs1024_verify_checksum(CS_SHAMIR, &full_data));
    }

    #[test]
    fn test_different_cs_fails() {
        let data: Vec<u16> = vec![100, 200, 300];
        let checksum = rs1024_create_checksum(CS_SHAMIR, &data);

        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        // Using wrong customization string should fail
        assert!(!rs1024_verify_checksum(CS_SHAMIR_EXTENDABLE, &full_data));
    }

    #[test]
    fn test_extendable_cs() {
        let data: Vec<u16> = vec![512, 256, 128, 64, 32, 16, 8];
        let checksum = rs1024_create_checksum(CS_SHAMIR_EXTENDABLE, &data);

        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        assert!(rs1024_verify_checksum(CS_SHAMIR_EXTENDABLE, &full_data));
        assert!(!rs1024_verify_checksum(CS_SHAMIR, &full_data));
    }

    #[test]
    fn test_three_word_error_detection() {
        // RS1024 guarantees detection of any error affecting at most 3 words
        let data: Vec<u16> = vec![100, 200, 300, 400, 500, 600, 700, 800];
        let checksum = rs1024_create_checksum(CS_SHAMIR, &data);

        let mut full_data = data.clone();
        full_data.extend_from_slice(&checksum);

        // Corrupt 1 word
        let mut corrupted = full_data.clone();
        corrupted[0] = (corrupted[0] + 1) % 1024;
        assert!(!rs1024_verify_checksum(CS_SHAMIR, &corrupted));

        // Corrupt 2 words
        let mut corrupted = full_data.clone();
        corrupted[0] = (corrupted[0] + 1) % 1024;
        corrupted[3] = (corrupted[3] + 1) % 1024;
        assert!(!rs1024_verify_checksum(CS_SHAMIR, &corrupted));

        // Corrupt 3 words
        let mut corrupted = full_data.clone();
        corrupted[0] = (corrupted[0] + 1) % 1024;
        corrupted[3] = (corrupted[3] + 1) % 1024;
        corrupted[6] = (corrupted[6] + 1) % 1024;
        assert!(!rs1024_verify_checksum(CS_SHAMIR, &corrupted));
    }
}
