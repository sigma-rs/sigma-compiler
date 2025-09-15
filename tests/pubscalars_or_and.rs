#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn pubscalars_or_and_test_val(x_val: u128, b_val: u128) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, rand r, pub a, pub b),
        (C, const cind A, const cind B),
        C = x*A + r*B,
        OR (
            AND (
                b = 2*a,
                x = 1,
            ),
            AND (
                b = 2*a - 3,
                x = 2,
            )
        )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let x = Scalar::from_u128(x_val);
    let a = Scalar::from_u128(7);
    let b = Scalar::from_u128(b_val);
    let C = x * A + r * B;

    let instance = proof::Instance { C, A, B, a, b };
    let witness = proof::Witness { x, r };

    let proof = proof::prove(&instance, &witness, b"pubscalars_or_and_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubscalars_or_and_test")
}

#[test]
fn pubscalars_or_test() {
    pubscalars_or_and_test_val(1u128, 10u128).unwrap_err();
    pubscalars_or_and_test_val(1u128, 11u128).unwrap_err();
    pubscalars_or_and_test_val(1u128, 12u128).unwrap_err();
    pubscalars_or_and_test_val(1u128, 13u128).unwrap_err();
    pubscalars_or_and_test_val(1u128, 14u128).unwrap();
    pubscalars_or_and_test_val(1u128, 15u128).unwrap_err();

    pubscalars_or_and_test_val(2u128, 10u128).unwrap_err();
    pubscalars_or_and_test_val(2u128, 11u128).unwrap();
    pubscalars_or_and_test_val(2u128, 12u128).unwrap_err();
    pubscalars_or_and_test_val(2u128, 13u128).unwrap_err();
    pubscalars_or_and_test_val(2u128, 14u128).unwrap_err();
    pubscalars_or_and_test_val(2u128, 15u128).unwrap_err();
}
