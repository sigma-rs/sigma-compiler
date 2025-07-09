#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn range_test() -> Result<(), sigma_rs::errors::Error> {
    sigma_compiler! { proof,
        (x, y, pub a, rand r),
        (C, D, const cind A, const cind B),
        C = (x*3+1)*A + (r*2+3)*B,
        D = x*A + y*B,
        (a..20).contains(x),
        (0..a).contains(y),
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let a = Scalar::from_u128(3);
    let r = Scalar::random(&mut rng);
    let x = Scalar::from_u128(19);
    let y = Scalar::from_u128(2);
    let C = (x + x + x + Scalar::ONE) * A + (r + r + Scalar::from_u128(3)) * B;
    let D = x * A + y * B;

    let params = proof::Params { C, D, a, A, B };
    let witness = proof::Witness { x, y, r };

    let proof = proof::prove(&params, &witness, b"range_test", &mut rng)?;
    proof::verify(&params, &proof, b"range_test")
}
