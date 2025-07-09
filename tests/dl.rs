#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::Group;
use sigma_compiler::*;

#[test]
fn dl_zero_test() -> Result<(), sigma_rs::errors::Error> {
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

    let params = proof::Params { C, B };
    let witness = proof::Witness { x };

    let proof = proof::prove(&params, &witness, b"dl_test", &mut rng)?;
    proof::verify(&params, &proof, b"dl_test")
}

#[test]
fn dl_one_test() -> Result<(), sigma_rs::errors::Error> {
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

    let params = proof::Params { C, B };
    let witness = proof::Witness { x };

    let proof = proof::prove(&params, &witness, b"dl_test", &mut rng)?;
    proof::verify(&params, &proof, b"dl_test")
}
