//! Signed reference geometry: logical areas that may extend beyond the worksheet grid,
//! their source-compatible dimensions, and projection onto physical cell reads.
//!
//! Depends on: values (physical areas and value channels).

/// Physical projection of one logical reference read.
mod grid_read;

/// Logical signed reference rectangles.
mod signed_area;

/// Source-compatible signed axis dimensions.
mod source_dimension;

#[cfg(test)]
mod tests;

pub(in crate::eval) use grid_read::GridRead;
pub(in crate::eval) use signed_area::SignedArea;
