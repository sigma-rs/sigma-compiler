#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn basic_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, z, rand r, rand s),
        (C, D, const cind A, const cind B),
        C = x*A + r*B,
        D = z*A + s*B,
        z = 2*x + 1,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let s = Scalar::random(&mut rng);
    let x = Scalar::from_u128(5);
    let z = Scalar::from_u128(11);
    let C = x * A + r * B;
    let D = z * A + s * B;

    let instance = proof::Instance { C, D, A, B };
    let witness = proof::Witness { x, z, r, s };

    let proof = proof::prove(&instance, &witness, b"basic_test", &mut rng)?;
    proof::verify(&instance, &proof, b"basic_test")
}
