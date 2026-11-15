//! Why the formatter (stage 6) refused a file. Each variant prints the text
//! the stage reported before the error was typed: a refused file's message
//! reaches the run's warnings, so the words are part of the output.

use super::ast::Field;
use crate::libdetect::function_carry::CarryError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The parser's first diagnostic.
    Parse(String),
    /// Syntax the converter does not port (TypeScript, decorators, …).
    Unsupported(&'static str),
    /// A statement or expression kind the converter does not know.
    UnsupportedAt {
        what: &'static str,
        start: u32,
        end: u32,
    },
    /// `accessor x`, which Babel's parser refuses without a plugin.
    AutoAccessor { at: u32 },
    /// A traversal or visitor step Babel would throw on, or one no stage-6
    /// visitor reaches.
    Traverse(&'static str),
    /// A single-node field this node kind does not have.
    NoField { kind: &'static str, field: Field },
    /// A list field this node kind does not have.
    NoListField(Field),
    /// Babel's `validate`: a node or null written into a list field.
    ExpectedArray {
        field: Field,
        kind: &'static str,
        got_node: bool,
    },
    /// Babel's `validate`: null written into a required field.
    ExpectedNode { field: Field, kind: &'static str },
    /// Fewer arena nodes than Babel scopes that declare `undefined`.
    UndefinedScopes { wanted: usize, matched: usize },
    /// The library carry over the formatted tree failed.
    Carry(CarryError),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Unsupported(what) | Self::Traverse(what) => f.write_str(what),
            Self::UnsupportedAt { what, start, end } => {
                write!(f, "unsupported {what} at {start}..{end}")
            }
            Self::AutoAccessor { at } => write!(
                f,
                "Support for the experimental syntax 'decoratorAutoAccessors' isn't currently enabled at {at}"
            ),
            Self::NoField { kind, field } => write!(f, "{kind} has no field {field:?}"),
            Self::NoListField(field) => write!(f, "no list field {field:?}"),
            Self::ExpectedArray {
                field,
                kind,
                got_node,
            } => write!(
                f,
                "Property {field:?} of {kind} expected type of array but got {}",
                if *got_node { "object" } else { "null" }
            ),
            Self::ExpectedNode { field, kind } => write!(
                f,
                "Property {field:?} of {kind} expected a node but instead got null"
            ),
            Self::UndefinedScopes { wanted, matched } => write!(
                f,
                "undefined-binding scopes: {wanted} Babel scope(s), {matched} matched in the tree"
            ),
            Self::Carry(e) => e.fmt(f),
        }
    }
}

impl From<CarryError> for FormatError {
    fn from(e: CarryError) -> Self {
        Self::Carry(e)
    }
}
