#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::ff::PrimeField;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn left_expr_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (x, y, pub a, rand r, rand s),
        (C, D, const cind A, const cind B),
        D = y*A + s*B,
        (2*a-1)*C + D = x*A + r*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let r = Scalar::random(&mut rng);
    let s = Scalar::random(&mut rng);
    let a = Scalar::from_u128(9);
    let x = Scalar::from_u128(5);
    let y = Scalar::from_u128(12);
    let D = y * A + s * B;
    let C = (x * A + r * B - D) * (a + a - Scalar::ONE).invert();

    let instance = proof::Instance { C, D, A, B, a };
    let witness = proof::Witness { x, y, r, s };

    let proof = proof::prove(&instance, &witness, b"left_expr_test", &mut rng)?;
    proof::verify(&instance, &proof, b"left_expr_test")
}

#[test]
fn left_expr_vec_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { proof,
        (vec x, vec y, z, pub vec a, pub b, rand vec r, rand vec s, rand t),
        (vec C, vec D, E, const cind A, const cind B),
        E = z*A + t*B,
        D = y*A + s*B,
        (2*a-1)*C + b*D + E = x*A + r*B,
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let A = G::hash_from_bytes::<Sha512>(b"Generator A");
    let B = G::generator();
    let vlen = 5usize;
    let r: Vec<Scalar> = (0..vlen).map(|_| Scalar::random(&mut rng)).collect();
    let s: Vec<Scalar> = (0..vlen).map(|_| Scalar::random(&mut rng)).collect();
    let t = Scalar::random(&mut rng);
    let a: Vec<Scalar> = (0..vlen).map(|i| Scalar::from_u128(i as u128)).collect();
    let b = Scalar::from_u128(17);
    let x: Vec<Scalar> = (0..vlen)
        .map(|i| Scalar::from_u128((2 * i) as u128))
        .collect();
    let y: Vec<Scalar> = (0..vlen)
        .map(|i| Scalar::from_u128((3 * i) as u128))
        .collect();
    let z = Scalar::from_u128(12);
    let E = z * A + t * B;
    let D: Vec<G> = (0..vlen).map(|i| y[i] * A + s[i] * B).collect();
    let C: Vec<G> = (0..vlen)
        .map(|i| (x[i] * A + r[i] * B - b * D[i] - E) * (a[i] + a[i] - Scalar::ONE).invert())
        .collect();

    let instance = proof::Instance {
        C,
        D,
        E,
        A,
        B,
        a,
        b,
    };
    let witness = proof::Witness { x, y, z, r, s, t };

    let proof = proof::prove(&instance, &witness, b"left_expr_vec_test", &mut rng)?;
    proof::verify(&instance, &proof, b"left_expr_vec_test")
}
