//! Galois Field GF(256) arithmetic for Shamir's Secret Sharing
//!
//! Uses the irreducible polynomial x^8 + x^4 + x^3 + x + 1 (0x11B)
//! This is the same field used by AES and SLIP-39.

/// Precomputed log table (log[x] = discrete log of x, log[0] is undefined)
static LOG: [u8; 256] = [
    0, 0, 1, 25, 2, 50, 26, 198, 3, 223, 51, 238, 27, 104, 199, 75, 4, 100, 224, 14, 52, 141, 239,
    129, 28, 193, 105, 248, 200, 8, 76, 113, 5, 138, 101, 47, 225, 36, 15, 33, 53, 147, 142, 218,
    240, 18, 130, 69, 29, 181, 194, 125, 106, 39, 249, 185, 201, 154, 9, 120, 77, 228, 114, 166, 6,
    191, 139, 98, 102, 221, 48, 253, 226, 152, 37, 179, 16, 145, 34, 136, 54, 208, 148, 206, 143,
    150, 219, 189, 241, 210, 19, 92, 131, 56, 70, 64, 30, 66, 182, 163, 195, 72, 126, 110, 107, 58,
    40, 84, 250, 133, 186, 61, 202, 94, 155, 159, 10, 21, 121, 43, 78, 212, 229, 172, 115, 243,
    167, 87, 7, 112, 192, 247, 140, 128, 99, 13, 103, 74, 222, 237, 49, 197, 254, 24, 227, 165,
    153, 119, 38, 184, 180, 124, 17, 68, 146, 217, 35, 32, 137, 46, 55, 63, 209, 91, 149, 188, 207,
    205, 144, 135, 151, 178, 220, 252, 190, 97, 242, 86, 211, 171, 20, 42, 93, 158, 132, 60, 57,
    83, 71, 109, 65, 162, 31, 45, 67, 216, 183, 123, 164, 118, 196, 23, 73, 236, 127, 12, 111, 246,
    108, 161, 59, 82, 41, 157, 85, 170, 251, 96, 134, 177, 187, 204, 62, 90, 203, 89, 95, 176, 156,
    169, 160, 81, 11, 245, 22, 235, 122, 117, 44, 215, 79, 174, 213, 233, 230, 231, 173, 232, 116,
    214, 244, 234, 168, 80, 88, 175,
];

/// Precomputed exp table (exp[i] = g^i where g is a generator)
static EXP: [u8; 510] = [
    1, 2, 4, 8, 16, 32, 64, 128, 29, 58, 116, 232, 205, 135, 19, 38, 76, 152, 45, 90, 180, 117,
    234, 201, 143, 3, 6, 12, 24, 48, 96, 192, 157, 39, 78, 156, 37, 74, 148, 53, 106, 212, 181,
    119, 238, 193, 159, 35, 70, 140, 5, 10, 20, 40, 80, 160, 93, 186, 105, 210, 185, 111, 222, 161,
    95, 190, 97, 194, 153, 47, 94, 188, 101, 202, 137, 15, 30, 60, 120, 240, 253, 231, 211, 187,
    107, 214, 177, 127, 254, 225, 223, 163, 91, 182, 113, 226, 217, 175, 67, 134, 17, 34, 68, 136,
    13, 26, 52, 104, 208, 189, 103, 206, 129, 31, 62, 124, 248, 237, 199, 147, 59, 118, 236, 197,
    151, 51, 102, 204, 133, 23, 46, 92, 184, 109, 218, 169, 79, 158, 33, 66, 132, 21, 42, 84, 168,
    77, 154, 41, 82, 164, 85, 170, 73, 146, 57, 114, 228, 213, 183, 115, 230, 209, 191, 99, 198,
    145, 63, 126, 252, 229, 215, 179, 123, 246, 241, 255, 227, 219, 171, 75, 150, 49, 98, 196, 149,
    55, 110, 220, 165, 87, 174, 65, 130, 25, 50, 100, 200, 141, 7, 14, 28, 56, 112, 224, 221, 167,
    83, 166, 81, 162, 89, 178, 121, 242, 249, 239, 195, 155, 43, 86, 172, 69, 138, 9, 18, 36, 72,
    144, 61, 122, 244, 245, 247, 243, 251, 235, 203, 139, 11, 22, 44, 88, 176, 125, 250, 233, 207,
    131, 27, 54, 108, 216, 173, 71, 142, // Repeat for easy modular arithmetic
    1, 2, 4, 8, 16, 32, 64, 128, 29, 58, 116, 232, 205, 135, 19, 38, 76, 152, 45, 90, 180, 117, 234,
    201, 143, 3, 6, 12, 24, 48, 96, 192, 157, 39, 78, 156, 37, 74, 148, 53, 106, 212, 181, 119,
    238, 193, 159, 35, 70, 140, 5, 10, 20, 40, 80, 160, 93, 186, 105, 210, 185, 111, 222, 161, 95,
    190, 97, 194, 153, 47, 94, 188, 101, 202, 137, 15, 30, 60, 120, 240, 253, 231, 211, 187, 107,
    214, 177, 127, 254, 225, 223, 163, 91, 182, 113, 226, 217, 175, 67, 134, 17, 34, 68, 136, 13,
    26, 52, 104, 208, 189, 103, 206, 129, 31, 62, 124, 248, 237, 199, 147, 59, 118, 236, 197, 151,
    51, 102, 204, 133, 23, 46, 92, 184, 109, 218, 169, 79, 158, 33, 66, 132, 21, 42, 84, 168, 77,
    154, 41, 82, 164, 85, 170, 73, 146, 57, 114, 228, 213, 183, 115, 230, 209, 191, 99, 198, 145,
    63, 126, 252, 229, 215, 179, 123, 246, 241, 255, 227, 219, 171, 75, 150, 49, 98, 196, 149, 55,
    110, 220, 165, 87, 174, 65, 130, 25, 50, 100, 200, 141, 7, 14, 28, 56, 112, 224, 221, 167, 83,
    166, 81, 162, 89, 178, 121, 242, 249, 239, 195, 155, 43, 86, 172, 69, 138, 9, 18, 36, 72, 144,
    61, 122, 244, 245, 247, 243, 251, 235, 203, 139, 11, 22, 44, 88, 176, 125, 250, 233, 207, 131,
    27, 54, 108, 216, 173, 71, 142,
];

/// Add two elements in GF(256) (XOR)
#[inline]
pub fn gf_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Subtract two elements in GF(256) (same as add in characteristic 2)
#[inline]
pub fn gf_sub(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Multiply two elements in GF(256)
#[inline]
pub fn gf_mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let log_a = LOG[a as usize] as usize;
    let log_b = LOG[b as usize] as usize;
    EXP[log_a + log_b]
}

/// Divide two elements in GF(256)
#[inline]
pub fn gf_div(a: u8, b: u8) -> u8 {
    assert!(b != 0, "Division by zero in GF(256)");
    if a == 0 {
        return 0;
    }
    let log_a = LOG[a as usize] as usize;
    let log_b = LOG[b as usize] as usize;
    // Add 255 to handle negative result
    EXP[log_a + 255 - log_b]
}

/// Compute the inverse of an element in GF(256)
#[inline]
pub fn gf_inv(a: u8) -> u8 {
    assert!(a != 0, "Inverse of zero in GF(256)");
    EXP[255 - LOG[a as usize] as usize]
}

/// Evaluate a polynomial at a given x value
/// coefficients[0] is the constant term, coefficients[n-1] is the highest degree
pub fn poly_eval(coefficients: &[u8], x: u8) -> u8 {
    if coefficients.is_empty() {
        return 0;
    }

    // Use Horner's method for efficiency
    let mut result = 0u8;
    for &coef in coefficients.iter().rev() {
        result = gf_add(gf_mul(result, x), coef);
    }
    result
}

/// Lagrange interpolation to recover the secret at x=0
/// shares: Vec<(x, y)> where x is the share index and y is the share value
pub fn lagrange_interpolate(shares: &[(u8, u8)]) -> u8 {
    let mut secret = 0u8;

    for (i, &(xi, yi)) in shares.iter().enumerate() {
        let mut numerator = 1u8;
        let mut denominator = 1u8;

        for (j, &(xj, _)) in shares.iter().enumerate() {
            if i != j {
                // numerator *= (0 - xj) = xj (in GF(256), negation is identity)
                numerator = gf_mul(numerator, xj);
                // denominator *= (xi - xj)
                denominator = gf_mul(denominator, gf_sub(xi, xj));
            }
        }

        // Lagrange basis polynomial Li(0) = numerator / denominator
        let li = gf_div(numerator, denominator);
        // Add yi * Li(0) to the sum
        secret = gf_add(secret, gf_mul(yi, li));
    }

    secret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf_add() {
        assert_eq!(gf_add(0x53, 0xCA), 0x99);
        assert_eq!(gf_add(0, 0x53), 0x53);
        assert_eq!(gf_add(0x53, 0x53), 0); // a + a = 0 in GF(2^n)
    }

    #[test]
    fn test_gf_mul() {
        assert_eq!(gf_mul(0, 0x53), 0);
        assert_eq!(gf_mul(1, 0x53), 0x53);
        // Test generator: 2 * 2 = 4
        assert_eq!(gf_mul(2, 2), 4);
        // Test overflow case: 0x80 * 2 should reduce mod the polynomial
        assert_eq!(gf_mul(128, 2), 29); // 0x80 * 2 = 0x100, reduced = 29 (0x1D)
    }

    #[test]
    fn test_gf_div() {
        assert_eq!(gf_div(0x53, 0x53), 1);
        assert_eq!(gf_div(0, 0x53), 0);
        // a / b * b = a
        let a = 0x53u8;
        let b = 0xCAu8;
        assert_eq!(gf_mul(gf_div(a, b), b), a);
    }

    #[test]
    fn test_gf_inv() {
        // a * inv(a) = 1
        for a in 1..=255u8 {
            assert_eq!(gf_mul(a, gf_inv(a)), 1, "Failed for a={}", a);
        }
    }

    #[test]
    fn test_poly_eval() {
        // p(x) = 5 + 3x + 2x^2
        let coeffs = [5u8, 3, 2];
        // p(0) = 5
        assert_eq!(poly_eval(&coeffs, 0), 5);
        // p(1) = 5 ^ 3 ^ 2 = 4 (XOR in GF(256))
        assert_eq!(poly_eval(&coeffs, 1), 4);
    }

    #[test]
    fn test_lagrange_simple() {
        // Simple test: secret=42, coefficient=7
        // p(x) = 42 + 7*x
        // p(1) = 42 ^ 7 = 45
        // p(2) = 42 ^ (7*2) = 42 ^ 14 = 36
        // p(3) = 42 ^ (7*3) = 42 ^ (7*3 in GF) = 42 ^ 9 = 35

        let secret = 42u8;
        let coef = 7u8;

        // Generate shares
        let shares: Vec<(u8, u8)> = (1..=3)
            .map(|x| (x, gf_add(secret, gf_mul(coef, x))))
            .collect();

        // Use any 2 shares to recover
        let recovered = lagrange_interpolate(&shares[0..2]);
        assert_eq!(recovered, secret);

        let recovered = lagrange_interpolate(&shares[1..3]);
        assert_eq!(recovered, secret);

        let recovered = lagrange_interpolate(&[shares[0], shares[2]]);
        assert_eq!(recovered, secret);
    }
}
