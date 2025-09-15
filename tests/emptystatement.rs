#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use sigma_compiler::*;

#[test]
fn emptystatement_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (),
        (),
    }

    let mut rng = rand::thread_rng();

    let instance = proof::Instance {};
    let witness = proof::Witness {};

    let proof = proof::prove(&instance, &witness, b"emptystatement_test", &mut rng)?;
    proof::verify(&instance, &proof, b"emptystatement_test")
}
