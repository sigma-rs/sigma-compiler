#![cfg(feature = "dump")]
#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn range_dump_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, y, pub a, rand r),
        (C, D, const cind A, const cind B),
        C = (3*x+1)*A + (2*r+3)*B,
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

    let instance = proof::Instance { C, D, a, A, B };
    let witness = proof::Witness { x, y, r };

    sigma_compiler::dumper::dump_to_string();

    let proof = proof::prove(&instance, &witness, b"range_test", &mut rng)?;
    let res = proof::verify(&instance, &proof, b"range_test");

    let buf = sigma_compiler::dumper::dump_buffer();
    print!("{buf}");

    res
}
