#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn disj_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, rand r),
        (C, const cind A, const cind B),
        C = (3*x+1)*A + (2*r+3)*B,
        OR (
            x=1,
            x=2,
        )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let x = Scalar::from_u128(1);
    let C = (x + x + x + Scalar::ONE) * A + (r + r + Scalar::from_u128(3)) * B;

    let instance = proof::Instance { C, A, B };
    let witness = proof::Witness { x, r };

    let proof = proof::prove(&instance, &witness, b"disj_test", &mut rng)?;
    proof::verify(&instance, &proof, b"disj_test")?;
    Ok(())
}
