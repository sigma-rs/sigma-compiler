#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn pubscalars_or_vec_test_vecsize_val(
    vecsize: usize,
    b_val: u128,
    x_val: Option<u128>,
) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, vec y, pub vec a, pub vec b, rand vec r, rand vec s),
        (vec C, vec D, const cind A, const cind B),
        C = x*A + r*B,
        D = y*A + s*B,
        OR (
            AND (
                b = 2*a,
                x = 1,
            ),
            AND (
                b = 2*a - 3,
                x = y,
            )
        )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let s: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let y: Vec<Scalar> = (0..vecsize).map(|i| Scalar::from_u128(i as u128)).collect();
    let x: Vec<Scalar> = (0..vecsize)
        .map(|i| {
            if let Some(xv) = x_val {
                Scalar::from_u128(xv)
            } else {
                y[i]
            }
        })
        .collect();
    let a: Vec<Scalar> = (0..vecsize)
        .map(|i| Scalar::from_u128((i + 12) as u128))
        .collect();
    let b: Vec<Scalar> = (0..vecsize)
        .map(|i| a[i] + a[i] - Scalar::from_u128(b_val))
        .collect();
    let C: Vec<G> = (0..vecsize).map(|i| x[i] * A + r[i] * B).collect();
    let D: Vec<G> = (0..vecsize).map(|i| y[i] * A + s[i] * B).collect();

    let instance = proof::Instance { C, D, A, B, a, b };
    let witness = proof::Witness { x, y, r, s };

    let proof = proof::prove(&instance, &witness, b"pubscalars_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"pubscalars_vec_test")
}

fn pubscalars_or_vec_emptyvec() {
    pubscalars_or_vec_test_vecsize_val(0, 0, Some(0)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 1, Some(0)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 2, Some(0)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 3, Some(0)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 4, Some(0)).unwrap();

    pubscalars_or_vec_test_vecsize_val(0, 0, Some(1)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 1, Some(1)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 2, Some(1)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 3, Some(1)).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 4, Some(1)).unwrap();

    pubscalars_or_vec_test_vecsize_val(0, 0, None).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 1, None).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 2, None).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 3, None).unwrap();
    pubscalars_or_vec_test_vecsize_val(0, 4, None).unwrap();
}

fn pubscalars_or_vec_vecsize(vecsize: usize) {
    pubscalars_or_vec_test_vecsize_val(vecsize, 0, Some(0)).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 1, Some(0)).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 2, Some(0)).unwrap_err();
    if vecsize == 1 {
        pubscalars_or_vec_test_vecsize_val(vecsize, 3, Some(0)).unwrap();
    } else {
        pubscalars_or_vec_test_vecsize_val(vecsize, 3, Some(0)).unwrap_err();
    }
    pubscalars_or_vec_test_vecsize_val(vecsize, 4, Some(0)).unwrap_err();

    pubscalars_or_vec_test_vecsize_val(vecsize, 0, Some(1)).unwrap();
    pubscalars_or_vec_test_vecsize_val(vecsize, 1, Some(1)).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 2, Some(1)).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 3, Some(1)).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 4, Some(1)).unwrap_err();

    pubscalars_or_vec_test_vecsize_val(vecsize, 0, None).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 1, None).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 2, None).unwrap_err();
    pubscalars_or_vec_test_vecsize_val(vecsize, 3, None).unwrap();
    pubscalars_or_vec_test_vecsize_val(vecsize, 4, None).unwrap_err();
}

#[test]
fn pubscalars_or_and_vec_0_test() {
    pubscalars_or_vec_emptyvec();
}

#[test]
fn pubscalars_or_and_vec_1_test() {
    pubscalars_or_vec_vecsize(1);
}

#[test]
fn pubscalars_or_and_vec_2_test() {
    pubscalars_or_vec_vecsize(2);
}

#[test]
fn pubscalars_or_and_vec_3_test() {
    pubscalars_or_vec_vecsize(3);
}
