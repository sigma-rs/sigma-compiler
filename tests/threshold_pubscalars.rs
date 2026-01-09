#![allow(non_snake_case)]
use curve25519_dalek::ristretto::RistrettoPoint as G;
use group::Group;
use sha2::Sha512;
use sigma_compiler::*;

#[test]
fn threshold_pubscalars_test() -> sigma_proofs::errors::Result<()> {
    sigma_compiler! { thresh3,
        (pub x1, pub x2, pub x3, pub x4, pub x5, rand r),
        (C, const cind G0, const cind G1, const cind G2, const cind G3,
            const cind G4, const cind G5),
        C = r*G0 + x1*G1 + x2*G2 + x3*G3 + x4*G4 + x5*G5,
        THRESH ( 3, x1 = 1, x2 = 2, x3 = 3, x4 = 4, x5 = 5 )
    }

    type Scalar = <G as Group>::Scalar;
    let mut rng = rand::thread_rng();
    let G0 = G::generator();
    let G1 = G::hash_from_bytes::<Sha512>(b"Generator G1");
    let G2 = G::hash_from_bytes::<Sha512>(b"Generator G2");
    let G3 = G::hash_from_bytes::<Sha512>(b"Generator G3");
    let G4 = G::hash_from_bytes::<Sha512>(b"Generator G4");
    let G5 = G::hash_from_bytes::<Sha512>(b"Generator G5");
    let r = Scalar::random(&mut rng);
    let y = Scalar::random(&mut rng);

    // Iterate over all combinations of 5 bits
    for true_pattern in 0u32..32 {
        let x1 = Scalar::from(if true_pattern & 1 == 0 { 2u32 } else { 1u32 });
        let x2 = Scalar::from(if true_pattern & 2 == 0 { 3u32 } else { 2u32 });
        let x3 = Scalar::from(if true_pattern & 4 == 0 { 4u32 } else { 3u32 });
        let x4 = Scalar::from(if true_pattern & 8 == 0 { 5u32 } else { 4u32 });
        let x5 = Scalar::from(if true_pattern & 16 == 0 { 6u32 } else { 5u32 });
        let C = r * G0 + x1 * G1 + x2 * G2 + x3 * G3 + x4 * G4 + x5 * G5;

        let num_true = true_pattern.count_ones();

        let instance = thresh3::Instance {
            C,
            G0,
            G1,
            G2,
            G3,
            G4,
            G5,
            x1,
            x2,
            x3,
            x4,
            x5,
        };
        let witness = thresh3::Witness { r };

        match thresh3::prove(&instance, &witness, b"thresh_pubscalars_test", &mut rng) {
            Ok(_) if num_true < 3 => {
                panic!("THRESH passed when it should have failed (true_pattern = {true_pattern})")
            }
            Err(_) if num_true >= 3 => {
                panic!("THRESH failed when it should have passed (true_pattern = {true_pattern})")
            }
            Ok(proof) => {
                thresh3::verify(&instance, &proof, b"thresh_pubscalars_test")?;
            }
            Err(_) => {}
        }
    }
    Ok(())
}
