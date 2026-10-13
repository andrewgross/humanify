//! The hash forms (WP1.3; named and versioned since the matching refactor's
//! step 2): every canonical "is this the same code?" form is a named
//! setting in [`forms`] — the match key (blurred literals; cross-version
//! matching), the identity key (verbatim literals; same-release identity),
//! the statement hash (the split's rename-invariant statement identity, its
//! own masked walk), the library units, the vendor content key and the
//! close tier's align forms — and [`forms::FORMS`] lists them with their
//! versions. TS originals: analysis/structural-hash.ts,
//! split/statement-hash.ts, analysis/enclosing-statement.ts.

pub mod forms;
pub mod serialize;
pub mod statement_hash;
pub mod wrapper_spelling;

#[cfg(test)]
mod forms_test;
#[cfg(test)]
mod serialize_test;
#[cfg(test)]
mod statement_hash_test;
