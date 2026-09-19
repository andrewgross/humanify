//! The three hash key types (02 §2): what a correctness gate consumes is
//! what it gets — a `MatchKey` passed where an `IdentityKey` is required
//! does not compile. This is the compile-time form of exp046's near-miss
//! (vendor byte-reuse keyed on the blurred hash would have shipped the
//! prior release's endpoints).

/// The literal-blurred structural hash (`structuralHash`): same-length
/// string swaps do not move it; number magnitudes only.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct MatchKey(pub [u8; 16]);

/// The literal-verbatim hash (`statementHash`): any literal byte moves it.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct IdentityKey(pub [u8; 16]);

/// The vendor factory's structural signature.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct VendorSignature(pub [u8; 16]);

impl MatchKey {
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
impl IdentityKey {
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
impl VendorSignature {
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
