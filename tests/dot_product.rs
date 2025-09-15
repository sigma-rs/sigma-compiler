#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::Group;
use sigma_compiler::*;

fn dot_product_test_vecsize(vecsize: usize) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, pub vec a),
        (C, D, E, F, vec A, B),
        C = sum(x*A),
        D = sum(a*A),
        E = sum(a*x*A),
        F = sum(a*x)*B,
        F = sum(a*x*B),
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A: Vec<G> = (0..vecsize).map(|_| G::random(&mut rng)).collect();
    let B = G::generator();
    let x: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let a: Vec<Scalar> = (0..vecsize).map(|_| Scalar::random(&mut rng)).collect();
    let C: G = (0..vecsize).map(|i| x[i] * A[i]).sum();
    let D: G = (0..vecsize).map(|i| a[i] * A[i]).sum();
    let E: G = (0..vecsize).map(|i| a[i] * x[i] * A[i]).sum();
    let F: G = (0..vecsize).map(|i| a[i] * x[i] * B).sum();

    let instance = proof::Instance {
        C,
        D,
        E,
        F,
        A,
        B,
        a,
    };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"dot_product_test", &mut rng)?;
    proof::verify(&instance, &proof, b"dot_product_test")
}

#[test]
fn dot_product_test() {
    dot_product_test_vecsize(0).unwrap();
    dot_product_test_vecsize(1).unwrap();
    dot_product_test_vecsize(2).unwrap();
    dot_product_test_vecsize(20).unwrap();
}
