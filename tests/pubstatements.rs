#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sigma_compiler::*;

#[test]
fn pubstatements_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, pub a),
        (C, D, const cind B),
        C = a*x*B,
        D = a*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let B = G::generator();
    let x = Scalar::from_u128(5);
    let a = Scalar::from_u128(0);
    let C = a * x * B;
    let D = a * B;

    let instance = proof::Instance { C, D, B, a };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"pubstatements_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubstatements_test")
}
