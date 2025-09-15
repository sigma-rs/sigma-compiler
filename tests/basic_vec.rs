#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn basic_vec_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, rand vec r),
        (vec C, const cind A, const cind B),
        C = x*A + r*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let x: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let C: Vec<G> = (0..vecsize).map(|i| x[i] * A + r[i] * B).collect();

    let instance = proof::Instance { C, A, B };
    let witness = proof::Witness { x, r };

    let proof = proof::prove(&instance, &witness, b"basic_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"basic_vec_test")
}

#[test]
fn basic_vec_test() {
    basic_vec_test_vecsize(0).unwrap();
    basic_vec_test_vecsize(1).unwrap();
    basic_vec_test_vecsize(2).unwrap();
    basic_vec_test_vecsize(20).unwrap();
}
