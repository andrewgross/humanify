//! Why the formatter (stage 6) refused a file. Each variant prints the text
//! the stage reported before the error was typed: a refused file's message
//! reaches the run's warnings, so the words are part of the output.

use super::ast::Field;
use crate::libdetect::function_carry::CarryError;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The parser's first diagnostic.
    #[error("parse error: {0}")]
    Parse(String),
    /// Syntax the converter does not port (TypeScript, decorators, …).
    #[error("{0}")]
    Unsupported(&'static str),
    /// A statement or expression kind the converter does not know.
    #[error("unsupported {what} at {start}..{end}")]
    UnsupportedAt {
        what: &'static str,
        start: u32,
        end: u32,
    },
    /// `accessor x`, which Babel's parser refuses without a plugin.
    #[error(
        "Support for the experimental syntax 'decoratorAutoAccessors' isn't currently enabled at {at}"
    )]
    AutoAccessor { at: u32 },
    /// A traversal or visitor step Babel would throw on, or one no stage-6
    /// visitor reaches.
    #[error("{0}")]
    Traverse(&'static str),
    /// A single-node field this node kind does not have.
    #[error("{kind} has no field {field:?}")]
    NoField { kind: &'static str, field: Field },
    /// A list field this node kind does not have.
    #[error("no list field {0:?}")]
    NoListField(Field),
    /// Babel's `validate`: a node or null written into a list field.
    #[error(
        "Property {field:?} of {kind} expected type of array but got {}",
        if *got_node { "object" } else { "null" }
    )]
    ExpectedArray {
        field: Field,
        kind: &'static str,
        got_node: bool,
    },
    /// Babel's `validate`: null written into a required field.
    #[error("Property {field:?} of {kind} expected a node but instead got null")]
    ExpectedNode { field: Field, kind: &'static str },
    /// Fewer arena nodes than Babel scopes that declare `undefined`.
    #[error("undefined-binding scopes: {wanted} Babel scope(s), {matched} matched in the tree")]
    UndefinedScopes { wanted: usize, matched: usize },
    /// The library carry over the formatted tree failed.
    #[error(transparent)]
    Carry(#[from] CarryError),
}
