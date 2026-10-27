//! Bounded local XML selection for FILTERXML; no external resource resolution.
//!
//! Depends on: roxmltree (admission), sxd-document (DOM), sxd-xpath (XPath).

mod admission;
mod host;
mod query;
mod xml_query_error;
mod xpath_document;

#[cfg(test)]
mod tests;

pub use host::Host;
pub use query::select;
pub use xml_query_error::XmlQueryError;
