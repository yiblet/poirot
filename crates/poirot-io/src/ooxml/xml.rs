//! Streaming XML codecs shared by import and patch execution.
use super::package::Package;
use crate::IoError;
use quick_xml::{
    Reader, Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use std::io::Write;

/// Convert an XML codec failure to the I/O diagnostic channel.
pub(super) fn xml(e: impl std::fmt::Display) -> IoError {
    IoError::Xml(e.to_string())
}
/// Read and decode one optional attribute by its exact XML name.
pub(super) fn attr(
    reader: &Reader<&[u8]>,
    e: &BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, IoError> {
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(xml)?;
        if a.key.as_ref() == name {
            return Ok(Some(
                a.decoded_and_normalized_value(
                    quick_xml::XmlVersion::Implicit1_0,
                    reader.decoder(),
                )
                .map_err(xml)?
                .into_owned(),
            ));
        }
    }
    Ok(None)
}
/// Construct a streaming reader that always expands empty elements into Start/End pairs.
/// Consumers handle both `<row/>` and `<row></row>` through their Start/End arms.
pub(super) fn events(bytes: &[u8]) -> Reader<&[u8]> {
    let mut r = Reader::from_reader(bytes);
    r.config_mut().expand_empty_elements = true;
    r
}
/// Borrow a required package part or report its absence.
pub(super) fn part<'a>(p: &'a Package, name: &str) -> Result<&'a [u8], IoError> {
    p.get(name)
        .map(Vec::as_slice)
        .ok_or_else(|| IoError::Read(format!("missing package part {name}")))
}
/// Read element text and decode XML entities.
pub(super) fn text(
    reader: &mut Reader<&[u8]>,
    name: quick_xml::name::QName<'_>,
) -> Result<String, IoError> {
    let raw = reader.read_text(name).map_err(xml)?;
    Ok(
        quick_xml::escape::unescape(&raw.xml10_content().map_err(xml)?)
            .map_err(xml)?
            .into_owned(),
    )
}
/// Write one XML event using the shared I/O diagnostic conversion.
pub(super) fn write_event<W: Write>(
    writer: &mut Writer<W>,
    event: Event<'_>,
) -> Result<(), IoError> {
    writer.write_event(event).map_err(xml)
}
/// Write an escaped text-only XML element.
pub(super) fn element<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
) -> Result<(), IoError> {
    write_event(writer, Event::Start(BytesStart::new(name)))?;
    write_event(writer, Event::Text(BytesText::new(value)))?;
    write_event(writer, Event::End(BytesEnd::new(name)))
}
