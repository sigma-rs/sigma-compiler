#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn pubscalars_or_vec_test_vecsize_val(
    vecsize: usize,
    b_val: u128,
) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, pub vec a, pub vec b, rand vec r),
        (vec C, const cind A, const cind B),
        C = x*A + r*B,
        OR (
            b = 2*a,
            b = 2*a - 3,
        )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let x: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let a: Vec<Scalar> = (0..vecsize)
        .map(|i| Scalar::from_u128((i + 12) as u128))
        .collect();
    let b: Vec<Scalar> = (0..vecsize)
        .map(|i| a[i] + a[i] - Scalar::from_u128(b_val))
        .collect();
    let C: Vec<G> = (0..vecsize).map(|i| x[i] * A + r[i] * B).collect();

    let instance = proof::Instance { C, A, B, a, b };
    let witness = proof::Witness { x, r };

    let proof = proof::prove(&instance, &witness, b"pubscalars_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubscalars_vec_test")
}

#[test]
fn pubscalars_or_vec_test() {
    pubscalars_or_vec_test_vecsize_val(0, 0).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 1).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 2).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 3).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 4).unwrap();
    for vecsize in [1, 2, 20] {
        pubscalars_or_vec_test_vecsize_val(vecsize, 0).unwrap();
        pubscalars_or_vec_test_vecsize_val(vecsize, 1).unwrap_err();
        pubscalars_or_vec_test_vecsize_val(vecsize, 2).unwrap_err();
        pubscalars_or_vec_test_vecsize_val(vecsize, 3).unwrap();
        pubscalars_or_vec_test_vecsize_val(vecsize, 4).unwrap_err();
    }
}
