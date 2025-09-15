//! A module containing some utility functions useful for the runtime
//! processing of range statements.

use group::ff::PrimeField;
use sigma_proofs::errors::Error;
use subtle::Choice;

/// Convert a [`Scalar`] to an [`u128`], assuming it fits in an [`i128`]
/// and is nonnegative.  Also output the number of bits of the
/// [`Scalar`].  This version assumes that `s` is public, and so does
/// not need to run in constant time.
///
/// [`Scalar`]: https://docs.rs/group/0.13.0/group/trait.Group.html#associatedtype.Scalar
pub fn bit_decomp_vartime<S: PrimeField>(mut s: S) -> Option<(u128, u32)> {
    let mut val = 0u128;
    let mut bitnum = 0u32;
    let mut bitval = 1u128; // Invariant: bitval = 2^bitnum
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

/// Given a [`Scalar`] `upper` (strictly greater than 1), make a vector
/// of [`Scalar`]s with the property that a [`Scalar`] `x` can be
/// written as a sum of zero or more (distinct) elements of this vector
/// if and only if `0 <= x < upper`.
///
/// The strategy is to write x as a sequence of `nbits` bits, with one
/// twist: the low bits represent 2^0, 2^1, 2^2, etc., as usual.  But
/// the highest bit represents `upper-2^{nbits-1}` instead of the usual
/// `2^{nbits-1}`.  `nbits` will be the largest value for which
/// `2^{nbits-1}` is strictly less than `upper`.  For example, if
/// `upper` is 100, the bits represent 1, 2, 4, 8, 16, 32, 36.  A number
/// x can be represented as a sum of 0 or more elements of this sequence
/// if and only if `0 <= x < upper`.
///
/// It is assumed that `upper` is public, and so this function is not
/// constant time.
///
/// [`Scalar`]: https://docs.rs/group/0.13.0/group/trait.Group.html#associatedtype.Scalar
pub fn bitrep_scalars_vartime<S: PrimeField>(upper: S) -> Result<Vec<S>, Error> {
    // Get the `u128` value of `upper`, and its number of bits `nbits`
    let (upper_val, mut nbits) = bit_decomp_vartime(upper).ok_or(Error::VerificationFailure)?;

    // Ensure `nbits` is at least 2.
    if nbits < 2 {
        return Err(Error::VerificationFailure);
    }

    // If upper is exactly a power of 2, use one fewer bit
    if upper_val == 1u128 << (nbits - 1) {
        nbits -= 1;
    }

    // Make the vector of Scalars containing the represented value of
    // the bits
    Ok((0..nbits)
        .map(|i| {
            if i < nbits - 1 {
                S::from_u128(1u128 << i)
            } else {
                // Compute the represented value of the highest bit
                S::from_u128(upper_val - (1u128 << (nbits - 1)))
            }
        })
        .collect())
}

/// Given a vector of [`Scalar`]s as output by
/// [`bitrep_scalars_vartime`] and a private [`Scalar`] `x`, output a
/// vector of [`Choice`] (of the same length as the given
/// `bitrep_scalars` vector) such that `x` is the sum of the chosen
/// elements of `bitrep_scalars`.  This function should be constant time
/// in the value of `x`.  If `x` is not less than the `upper` used by
/// [`bitrep_scalars_vartime`] to generate `bitrep_scalars`, then `x`
/// will not (and indeed cannot) equal the sum of the chosen elements of
/// `bitrep_scalars`.
///
/// [`Scalar`]: https://docs.rs/group/0.13.0/group/trait.Group.html#associatedtype.Scalar
pub fn compute_bitrep<S: PrimeField>(mut x: S, bitrep_scalars: &[S]) -> Vec<Choice> {
    // We know the length of bitrep_scalars is at most 127.
    let nbits: u32 = bitrep_scalars.len().try_into().unwrap();

    // Decompose `x` as a normal `nbit`-bit vector.  This only looks at
    // the low `nbits` bits of `x`, so the resulting bit vector forces
    // `x < 2^{nbits}`.
    let x_raw_bits = bit_decomp(x, nbits);
    let high_bit = x_raw_bits[(nbits as usize) - 1];

    // Conditionally subtract the last represented value in the
    // vector, depending on whether the high bit of x is set.  That is,
    // if `x < 2^{nbits-1}`, then we don't subtract from x.  If `x >=
    // 2^{nbits-1}`, then we will subtract `upper - 2^{nbits-1}` from
    // `x`.  In either case, the remaining value is non-negative, and
    // strictly less than 2^{nbits-1}.
    x -= S::conditional_select(&S::ZERO, &bitrep_scalars[(nbits as usize) - 1], high_bit);

    // Now get the `nbits-1` bits of the result in the usual way
    let mut x_bits = bit_decomp(x, nbits - 1);

    // and tack on the high bit
    x_bits.push(high_bit);

    x_bits
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
            Some(((i128::MAX - 1) as u128, 127))
        );
        assert_eq!(
            bit_decomp_vartime(Scalar::from((1u128 << 127) - 1)),
            Some((i128::MAX as u128, 127))
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

    // Obliviously test whether x is in 0..upper (that is, 0 <= x <
    // upper) using bit decomposition.  `upper` is considered public,
    // but `x` is private.  `upper` must be at least 2.
    fn bitrep_tester(upper: Scalar, x: Scalar, expected: bool) -> Result<(), Error> {
        let rep_scalars = bitrep_scalars_vartime(upper)?;
        let bitrep = compute_bitrep(x, &rep_scalars);

        let nbits = bitrep.len();
        assert!(nbits == rep_scalars.len());
        let mut x_out = Scalar::ZERO;
        for i in 0..nbits {
            x_out += Scalar::conditional_select(&Scalar::ZERO, &rep_scalars[i], bitrep[i]);
        }

        if (x == x_out) != expected {
            return Err(Error::VerificationFailure);
        }

        Ok(())
    }

    #[test]
    fn bitrep_test() {
        bitrep_tester(Scalar::from(0u32), Scalar::from(0u32), false).unwrap_err();
        bitrep_tester(Scalar::from(1u32), Scalar::from(0u32), true).unwrap_err();
        bitrep_tester(Scalar::from(2u32), Scalar::from(1u32), true).unwrap();
        bitrep_tester(Scalar::from(3u32), Scalar::from(1u32), true).unwrap();
        bitrep_tester(Scalar::from(100u32), Scalar::from(99u32), true).unwrap();
        bitrep_tester(Scalar::from(127u32), Scalar::from(126u32), true).unwrap();
        bitrep_tester(Scalar::from(128u32), Scalar::from(127u32), true).unwrap();
        bitrep_tester(Scalar::from(128u32), Scalar::from(128u32), false).unwrap();
        bitrep_tester(Scalar::from(129u32), Scalar::from(128u32), true).unwrap();
        bitrep_tester(Scalar::from(129u32), Scalar::from(0u32), true).unwrap();
        bitrep_tester(Scalar::from(129u32), Scalar::from(129u32), false).unwrap();
    }
}
