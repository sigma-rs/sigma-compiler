#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn basic_sum_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, y, rand vec r, rand s),
        (vec C, D, const cind A, const cind B),
        C = x*A + r*B,
        D = y*A + s*B,
        y = sum(x),
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let s = Scalar::random(&mut rng);
    let x: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let y: Scalar = x.iter().sum();
    let C: Vec<G> = (0..vecsize).map(|i| x[i] * A + r[i] * B).collect();
    let D = y * A + s * B;

    let instance = proof::Instance { C, D, A, B };
    let witness = proof::Witness { x, y, r, s };

    let proof = proof::prove(&instance, &witness, b"basic_sum_test", &mut rng)?;
    proof::verify(&instance, &proof, b"basic_sum_test")
}

#[test]
fn basic_sum_test() {
    basic_sum_test_vecsize(0).unwrap();
    basic_sum_test_vecsize(1).unwrap();
    basic_sum_test_vecsize(2).unwrap();
    basic_sum_test_vecsize(20).unwrap();
}
