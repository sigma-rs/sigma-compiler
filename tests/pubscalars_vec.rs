#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn pubscalars_vec_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, vec y, pub vec a, pub vec b, rand vec r, rand vec s),
        (vec C, vec D, const cind A, const cind B),
        C = x*A + r*B,
        D = y*A + s*B,
        y = 2*x + b,
        b = 3*a - 7,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let s: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let x: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let a: Vec<Scalar> = (0..vecsize)
        .map(|i| Scalar::from_u128((i + 12) as u128))
        .collect();
    let b: Vec<Scalar> = (0..vecsize)
        .map(|i| a[i] + a[i] + a[i] - Scalar::from_u128(7))
        .collect();
    let y: Vec<Scalar> = (0..vecsize).map(|i| x[i] + x[i] + b[i]).collect();
    let C: Vec<G> = (0..vecsize).map(|i| x[i] * A + r[i] * B).collect();
    let D: Vec<G> = (0..vecsize).map(|i| y[i] * A + s[i] * B).collect();

    let instance = proof::Instance { C, D, A, B, a, b };
    let witness = proof::Witness { x, y, r, s };

    let proof = proof::prove(&instance, &witness, b"pubscalars_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubscalars_vec_test")
}

#[test]
fn pubscalars_vec_test() {
    pubscalars_vec_test_vecsize(0).unwrap();
    pubscalars_vec_test_vecsize(1).unwrap();
    pubscalars_vec_test_vecsize(2).unwrap();
    pubscalars_vec_test_vecsize(20).unwrap();
}
