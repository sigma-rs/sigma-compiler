# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-10-10

### Added

- Initial release

## [0.2.0] - 2026-03-25

### Added

- Support for the THRESH (threshold) combiner, in addition to AND and OR.
- The new `dump` feature outputs (to stdout or to a `String`) the value of the
  `Instance` struct on both the prover and the verifier.  They should
  match, so this feature is helpful in debugging cases where they're
  not matching.
- Broaden the places where `rand` Scalars can appear. Before, a `rand`
  Scalar could only appear one time in total over all of the statements
  in the ZKP.  If one appeared more than once, it would not be
  considered `rand` for the purposes of recognizing Pedersen
  commitments.  Now, `rand` Scalars can appear multiple times in linear
  combination statements, but cannot appear at all (with a compile-time
  error) in range or not-equals statements (where it never made sense
  for them to appear anyway).

### Changes

- Depend on `sigma-proofs` version 0.2.0, which allows us to generate shorter zero-knowledge proofs using its `prove_compact` functionality.

### Fixes

- Choose the variables for generated Pedersen commitments deterministically.


[0.1.0]: https://git-crysp.uwaterloo.ca/SigmaProtocol/sigma-compiler/src/0.1.0
[0.2.0]: https://git-crysp.uwaterloo.ca/SigmaProtocol/sigma-compiler/src/0.2.0
