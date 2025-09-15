#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn pubscalars_test_val(b_val: u128) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, z, rand r, rand s, pub a, pub b),
        (C, D, const cind A, const cind B),
        C = x*A + r*B,
        D = z*A + s*B,
        z = 2*x + a,
        b = 2*a - 3,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let s = Scalar::random(&mut rng);
    let x = Scalar::from_u128(5);
    let z = Scalar::from_u128(17);
    let a = Scalar::from_u128(7);
    let b = Scalar::from_u128(b_val);
    let C = x * A + r * B;
    let D = z * A + s * B;

    let instance = proof::Instance { C, D, A, B, a, b };
    let witness = proof::Witness { x, z, r, s };

    let proof = proof::prove(&instance, &witness, b"pubscalars_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubscalars_test")
}

#[test]
fn pubscalars_test() {
    pubscalars_test_val(10u128).unwrap_err();
    pubscalars_test_val(11u128).unwrap();
    pubscalars_test_val(12u128).unwrap_err();
    pubscalars_test_val(13u128).unwrap_err();
    pubscalars_test_val(14u128).unwrap_err();
    pubscalars_test_val(15u128).unwrap_err();
}
