#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sigma_compiler::*;

fn pubstatements_vec_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, pub vec a),
        (vec C, vec D, const cind B),
        C = a*x*B,
        D = a*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let B = G::generator();
    let a: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let x: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let C: Vec<G> = (0..vecsize).map(|i| a[i] * x[i] * B).collect();
    let D: Vec<G> = (0..vecsize).map(|i| a[i] * B).collect();

    let instance = proof::Instance { C, D, B, a };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"pubstatements_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubstatements_vec_test")
}

#[test]
fn pubstatements_vec_test() {
    pubstatements_vec_test_vecsize(0).unwrap();
    pubstatements_vec_test_vecsize(1).unwrap();
    pubstatements_vec_test_vecsize(2).unwrap();
    pubstatements_vec_test_vecsize(20).unwrap();
}
