//! A module containing some utility functions useful for the runtime
//! processing of range proofs.

use group::ff::PrimeField;
use subtle::Choice;

/// Convert a [`Scalar`] to an [`i128`], assuming it fits and is
/// nonnegative.  Also output the number of bits of the [`Scalar`].
/// This version assumes that `s` is public, and so does not need to run
/// in constant time.
///
/// [`Scalar`]: https://docs.rs/group/0.13.0/group/trait.Group.html#associatedtype.Scalar
pub fn bit_decomp_vartime<S: PrimeField>(mut s: S) -> Option<(i128, u32)> {
    let mut val = 0i128;
    let mut bitnum = 0u32;
    let mut bitval = 1i128; // Invariant: bitval = 2^bitnum
    while bitnum < 127 && !s.is_zero_vartime() {
        if s.is_odd().into() {
            val += bitval;
            s -= S::ONE;
        }
        bitnum += 1;
        bitval <<= 1;
        s *= S::TWO_INV;
    }
    if s.is_zero_vartime() {
        Some((val, bitnum))
    } else {
        None
    }
}

/// Convert the low `nbits` bits of the given [`Scalar`] to a vector of
/// [`Choice`].  The first element of the vector is the low bit.  This
/// version runs in constant time.
///
/// [`Scalar`]: https://docs.rs/group/0.13.0/group/trait.Group.html#associatedtype.Scalar
pub fn bit_decomp<S: PrimeField>(mut s: S, nbits: u32) -> Vec<Choice> {
    let mut bits = Vec::with_capacity(nbits as usize);
    let mut bitnum = 0u32;
    while bitnum < nbits && bitnum < 127 {
        let lowbit = s.is_odd();
        s -= S::conditional_select(&S::ZERO, &S::ONE, lowbit);
        s *= S::TWO_INV;
        bits.push(lowbit);
        bitnum += 1;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek::scalar::Scalar;
    use std::ops::Neg;
    use subtle::ConditionallySelectable;

    fn bit_decomp_tester(s: Scalar, nbits: u32, expect_bitstr: &str) {
        // Convert the expected string of '0' and '1' into a vector of
        // Choice
        assert_eq!(
            bit_decomp(s, nbits)
                .into_iter()
                .map(|c| char::from(u8::conditional_select(&b'0', &b'1', c)))
                .collect::<String>(),
            expect_bitstr
        );
    }

    #[test]
    fn bit_decomp_test() {
        assert_eq!(bit_decomp_vartime(Scalar::from(0u32)), Some((0, 0)));
        assert_eq!(bit_decomp_vartime(Scalar::from(1u32)), Some((1, 1)));
        assert_eq!(bit_decomp_vartime(Scalar::from(2u32)), Some((2, 2)));
        assert_eq!(bit_decomp_vartime(Scalar::from(3u32)), Some((3, 2)));
        assert_eq!(bit_decomp_vartime(Scalar::from(4u32)), Some((4, 3)));
        assert_eq!(bit_decomp_vartime(Scalar::from(5u32)), Some((5, 3)));
        assert_eq!(bit_decomp_vartime(Scalar::from(6u32)), Some((6, 3)));
        assert_eq!(bit_decomp_vartime(Scalar::from(7u32)), Some((7, 3)));
        assert_eq!(bit_decomp_vartime(Scalar::from(8u32)), Some((8, 4)));
        assert_eq!(bit_decomp_vartime(Scalar::from(1u32).neg()), None);
        assert_eq!(
            bit_decomp_vartime(Scalar::from((1u128 << 127) - 2)),
            Some((i128::MAX - 1, 127))
        );
        assert_eq!(
            bit_decomp_vartime(Scalar::from((1u128 << 127) - 1)),
            Some((i128::MAX, 127))
        );
        assert_eq!(bit_decomp_vartime(Scalar::from(1u128 << 127)), None);

        bit_decomp_tester(Scalar::from(0u32), 0, "");
        bit_decomp_tester(Scalar::from(0u32), 5, "00000");
        bit_decomp_tester(Scalar::from(1u32), 0, "");
        bit_decomp_tester(Scalar::from(1u32), 1, "1");
        bit_decomp_tester(Scalar::from(2u32), 1, "0");
        bit_decomp_tester(Scalar::from(2u32), 2, "01");
        bit_decomp_tester(Scalar::from(3u32), 1, "1");
        bit_decomp_tester(Scalar::from(3u32), 2, "11");
        bit_decomp_tester(Scalar::from(5u32), 8, "10100000");
        // The order of this Scalar group is
        // 0x1000000000000000000000000000000014def9dea2f79cd65812631a5cf5d3ed
        bit_decomp_tester(
            Scalar::from(1u32).neg(),
            32,
            "00110111110010111010111100111010",
        );
        bit_decomp_tester(Scalar::from((1u128 << 127) - 2), 127,
        "0111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"
        );
        bit_decomp_tester(Scalar::from((1u128 << 127) - 1), 127,
        "1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"
        );
        bit_decomp_tester(Scalar::from(1u128 << 127), 127,
        "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );
        bit_decomp_tester(Scalar::from(1u128 << 127), 128,
        "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );
    }
}
