//! A module containing some utility functions useful for the runtime
//! processing of vector operations.

use core::iter::Sum;
use std::ops::{Add, Mul, Sub};

/// Add two vectors componentwise
pub fn add_vecs<L, R, P>(left: &[L], right: &[R]) -> Vec<P>
where
    L: Add<R, Output = P> + Clone,
    R: Clone,
{
    left.iter()
        .cloned()
        .zip(right.iter().cloned())
        .map(|(l, r)| l + r)
        .collect()
}

/// Add components of a vector by a non-vector
pub fn add_vec_nv<L, R, P>(left: &[L], right: &R) -> Vec<P>
where
    L: Add<R, Output = P> + Clone,
    R: Clone,
{
    left.iter().cloned().map(|l| l + right.clone()).collect()
}

/// Add a non-vector by components of a vector
pub fn add_nv_vec<L, R, P>(left: &L, right: &[R]) -> Vec<P>
where
    L: Add<R, Output = P> + Clone,
    R: Clone,
{
    right.iter().cloned().map(|r| left.clone() + r).collect()
}

/// Subtract two vectors componentwise
pub fn sub_vecs<L, R, P>(left: &[L], right: &[R]) -> Vec<P>
where
    L: Sub<R, Output = P> + Clone,
    R: Clone,
{
    left.iter()
        .cloned()
        .zip(right.iter().cloned())
        .map(|(l, r)| l - r)
        .collect()
}

/// Subtract components of a vector by a non-vector
pub fn sub_vec_nv<L, R, P>(left: &[L], right: &R) -> Vec<P>
where
    L: Sub<R, Output = P> + Clone,
    R: Clone,
{
    left.iter().cloned().map(|l| l - right.clone()).collect()
}

/// Subtract a non-vector by components of a vector
pub fn sub_nv_vec<L, R, P>(left: &L, right: &[R]) -> Vec<P>
where
    L: Sub<R, Output = P> + Clone,
    R: Clone,
{
    right.iter().cloned().map(|r| left.clone() - r).collect()
}

/// Multiply two vectors componentwise
pub fn mul_vecs<L, R, P>(left: &[L], right: &[R]) -> Vec<P>
where
    L: Mul<R, Output = P> + Clone,
    R: Clone,
{
    left.iter()
        .cloned()
        .zip(right.iter().cloned())
        .map(|(l, r)| l * r)
        .collect()
}

/// Multiply components of a vector by a non-vector
pub fn mul_vec_nv<L, R, P>(left: &[L], right: &R) -> Vec<P>
where
    L: Mul<R, Output = P> + Clone,
    R: Clone,
{
    left.iter().cloned().map(|l| l * right.clone()).collect()
}

/// Multiply a non-vector by components of a vector
pub fn mul_nv_vec<L, R, P>(left: &L, right: &[R]) -> Vec<P>
where
    L: Mul<R, Output = P> + Clone,
    R: Clone,
{
    right.iter().cloned().map(|r| left.clone() * r).collect()
}

/// Add the elements of a vector.
///
/// This wrapper avoids the problem of the standard
/// [`sum`](Sum#tymethod.sum) function requiring you to explicitly
/// specify the output type.  This wrapper gives you whatever type you
/// get by adding two values of type `T` together.  `T` must be
/// [`Clone`] because we're adding things of type `T` and not `&T`.
pub fn sum_vec<T, S>(summable: &[T]) -> S
where
    T: Add<T, Output = S> + Clone,
    S: Sum<T>,
{
    summable.iter().cloned().sum()
}
