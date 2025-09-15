#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

fn do_test(x_u128: u128) -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, rand r),
        (C, const cind A, const cind B),
        C = (3*x+1)*A + r*B,
        2*x-5 != 1,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let x = Scalar::from_u128(x_u128);
    let C = (Scalar::from_u128(3) * x + Scalar::ONE) * A + r * B;

    let instance = proof::Instance { C, A, B };
    let witness = proof::Witness { x, r };

    let proof = proof::prove(&instance, &witness, b"notequals_test", &mut rng)?;
    proof::verify(&instance, &proof, b"notequals_test")
}

#[test]
fn notequals_test() {
    do_test(0).unwrap();
    do_test(1).unwrap();
    do_test(2).unwrap();
    do_test(4).unwrap();
    do_test(5).unwrap();
}

#[test]
#[should_panic]
fn notequals_fail_test() {
    do_test(3).unwrap();
}
