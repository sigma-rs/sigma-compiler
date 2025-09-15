#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::Group;
use sigma_compiler::*;

#[test]
fn dl_zero_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x),
        (C, const B),
        C = (x+0)*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let B = G::generator();
    let x = Scalar::random(&mut rng);
    let C = (x + Scalar::ZERO) * B;

    let instance = proof::Instance { C, B };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"dl_test", &mut rng)?;
    proof::verify(&instance, &proof, b"dl_test")
}

#[test]
fn dl_one_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x),
        (C, const B),
        C = (x+1)*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let B = G::generator();
    let x = Scalar::random(&mut rng);
    let C = (x + Scalar::ONE) * B;

    let instance = proof::Instance { C, B };
    let witness = proof::Witness { x };

    let proof = proof::prove(&instance, &witness, b"dl_test", &mut rng)?;
    proof::verify(&instance, &proof, b"dl_test")
}
