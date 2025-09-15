#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn subtract_vec_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x),
        (vec C, vec D, vec E, const cind A, const cind B),
        C = (x-1)*A,
        D = (x-2)*B - C,
        E = (x-2)*B - A,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let x: Vec<Scalar> = (0..vecsize)
        .map(|i| Scalar::from_u128((i + 5) as u128))
        .collect();
    let C: Vec<G> = (0..vecsize).map(|i| (x[i] - Scalar::ONE) * A).collect();
    let D: Vec<G> = (0..vecsize)
        .map(|i| (x[i] - Scalar::from_u128(2)) * B - C[i])
        .collect();
    let E: Vec<G> = (0..vecsize)
        .map(|i| (x[i] - Scalar::from_u128(2)) * B - A)
        .collect();

    let instance = proof::Instance { C, D, E, A, B };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"subtract_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"subtract_vec_test")
}

#[test]
fn subtract_vec_test() {
    subtract_vec_test_vecsize(0).unwrap();
    subtract_vec_test_vecsize(1).unwrap();
    subtract_vec_test_vecsize(2).unwrap();
    subtract_vec_test_vecsize(20).unwrap();
}
