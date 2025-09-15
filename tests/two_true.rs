#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn two_true_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, y),
        (C, D, const cind A, const cind B),
        OR (
            C = x*A,
            D = y*B,
        )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let x = Scalar::random(&mut rng);
    let y = Scalar::random(&mut rng);
    let C = x * A;
    let D = y * B;

    let instance = proof::Instance { C, D, A, B };
    let witness = proof::Witness { x, y };

    let proof = proof::prove(&instance, &witness, b"two_true_test", &mut rng)?;
    proof::verify(&instance, &proof, b"two_true_test")?;
    Ok(())
}
