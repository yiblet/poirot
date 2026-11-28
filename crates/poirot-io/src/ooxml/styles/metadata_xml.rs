use super::codec::valid;
use crate::ooxml::xml::xml;
use crate::{IoError, StyleDecodeError};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
    name::{NamespaceResolver, ResolveResult},
};

/// Validate metadata events as the style decoder visits or skips their records.
pub(super) struct MetadataXml<'a> {
    reader: Reader<&'a [u8]>,
    namespaces: NamespaceResolver,
    root: &'static str,
    namespace: &'static str,
    track_foreign: bool,
    in_namespace: bool,
    depth: usize,
    seen_root: bool,
    seen_declaration: bool,
    foreign: bool,
}

impl<'a> MetadataXml<'a> {
    /// Check the document root and optionally report foreign descendant namespaces.
    pub(super) fn new(
        bytes: &'a [u8],
        root: &'static str,
        namespace: &'static str,
        track_foreign: bool,
    ) -> Self {
        let mut reader = Reader::from_reader(bytes);
        reader.config_mut().expand_empty_elements = true;
        Self {
            reader,
            namespaces: NamespaceResolver::default(),
            root,
            namespace,
            track_foreign,
            in_namespace: false,
            depth: 0,
            seen_root: false,
            seen_declaration: false,
            foreign: false,
        }
    }

    /// Validate a complete metadata part whose decoded events are unused.
    pub(super) fn validate(mut self) -> Result<(), IoError> {
        while !matches!(self.read_event()?, Event::Eof) {}
        Ok(())
    }

    /// Return whether any descendant start tag was outside the expected namespace.
    pub(super) fn foreign(&self) -> bool {
        self.foreign
    }

    /// Return the namespace classification of the last start tag.
    pub(super) fn in_namespace(&self) -> bool {
        self.in_namespace
    }

    /// Walk past the current record while applying ordinary metadata validation.
    pub(super) fn skip_subtree(&mut self) -> Result<(), IoError> {
        let depth = self
            .depth
            .checked_sub(1)
            .ok_or(IoError::Style(StyleDecodeError::XmlDepth))?;
        loop {
            match self.read_event()? {
                Event::End(_) if self.depth == depth => return Ok(()),
                Event::Eof => return Err(IoError::Style(StyleDecodeError::TruncatedStyleRecord)),
                _ => {}
            }
        }
    }

    /// Read one event, validating its context and all XML characters or references.
    pub(super) fn read_event(&mut self) -> Result<Event<'a>, IoError> {
        let event = self.reader.read_event().map_err(xml)?;
        match &event {
            Event::Start(start) => {
                // The namespace lifecycle follows quick-xml's MIT-licensed NsReader
                // at revision 36a2c52a4f6c90878c3f60c3c3b5c62efaef28a9.
                // https://github.com/tafia/quick-xml/blob/36a2c52a4f6c90878c3f60c3c3b5c62efaef28a9/src/reader/ns_reader.rs
                // A declaration always contains the literal key prefix. Tags with
                // none still enter a scope, and their attributes are checked below.
                let empty = BytesStart::new("");
                let bindings = if start.attributes_raw().contains("xmlns") {
                    start
                } else {
                    &empty
                };
                self.namespaces
                    .push(bindings)
                    .map_err(quick_xml::Error::from)
                    .map_err(xml)?;
                if self.depth == 0 || self.track_foreign {
                    let (ns, name) = self.namespaces.resolve_element(start.name());
                    self.in_namespace =
                        matches!(ns, ResolveResult::Bound(n) if n.as_ref() == self.namespace);
                    if self.depth == 0 {
                        if self.seen_root || name.as_ref() != self.root || !self.in_namespace {
                            return Err(IoError::Style(StyleDecodeError::MetadataRootNamespace));
                        }
                        self.seen_root = true;
                    } else {
                        self.foreign |= !self.in_namespace;
                    }
                }
                self.depth = self
                    .depth
                    .checked_add(1)
                    .ok_or(IoError::Style(StyleDecodeError::XmlDepth))?;
                for attribute in start.attributes() {
                    let attribute = attribute.map_err(xml)?;
                    // These bytes are legal XML characters. Excluding the entity
                    // introducer also rules out errors from reference expansion.
                    // The normalized value itself is unused during validation.
                    if !attribute
                        .value
                        .bytes()
                        .all(|byte| matches!(byte, b' '..=0x7f if byte != b'&'))
                    {
                        let value = attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(xml)?;
                        valid(&value)?;
                    }
                }
            }
            Event::End(_) => {
                self.depth = self
                    .depth
                    .checked_sub(1)
                    .ok_or(IoError::Style(StyleDecodeError::XmlDepth))?;
                self.namespaces.pop();
            }
            Event::DocType(_) => return Err(IoError::Style(StyleDecodeError::DtdInMetadata)),
            Event::Decl(_) => {
                if self.seen_root || self.seen_declaration {
                    return Err(IoError::Style(StyleDecodeError::MisplacedXmlDeclaration));
                }
                self.seen_declaration = true;
            }
            Event::CData(text) => {
                if self.depth == 0 {
                    return Err(IoError::Style(StyleDecodeError::CdataOutsideMetadataRoot));
                }
                valid(text.as_ref())?;
            }
            Event::GeneralRef(reference) => {
                if self.depth == 0 {
                    return Err(IoError::Style(
                        StyleDecodeError::ReferenceOutsideMetadataRoot,
                    ));
                }
                match reference.resolve_char_ref().map_err(xml)? {
                    // SAFETY: The four-byte array can hold any UTF-8 encoded char.
                    Some(ch) => valid(ch.encode_utf8(&mut [0; 4]))?,
                    None if matches!(reference.as_ref(), "amp" | "lt" | "gt" | "apos" | "quot") => {
                    }
                    None => return Err(IoError::Style(StyleDecodeError::UnknownEntityInMetadata)),
                }
            }
            Event::Text(text) => {
                let value = text.xml10_content();
                valid(&value)?;
                if self.depth == 0 && !value.trim().is_empty() {
                    return Err(IoError::Style(StyleDecodeError::TextOutsideMetadataRoot));
                }
            }
            Event::Eof if !self.seen_root || self.depth != 0 => {
                return Err(IoError::Style(StyleDecodeError::IncompleteMetadataRoot));
            }
            _ => {}
        }
        Ok(event)
    }
}
