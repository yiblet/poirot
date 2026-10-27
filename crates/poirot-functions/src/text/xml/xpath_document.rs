use super::xml_query_error::XmlQueryError;
use std::collections::BTreeMap;

/// Build the XPath document tree from XML that roxmltree has validated and normalized.
/// Copying this tree avoids SXD's parser splitting text at entity boundaries.
/// XPath 1.0 requires adjacent text to form one node; this also keeps node counts stable.
pub(super) struct XpathDocument(sxd_document::Package);

impl XpathDocument {
    /// Copy a validated document without recursion, preserving normalized XML text.
    pub(super) fn new<E>(source: &roxmltree::Document<'_>) -> Result<Self, XmlQueryError<E>> {
        let package = sxd_document::Package::new();
        let target = package.as_document();
        let mut elements = BTreeMap::new();
        for node in source.descendants() {
            if node.is_root() {
                continue;
            }
            let parent = node.parent().ok_or(XmlQueryError::Invalid)?;
            let parent_element: Option<sxd_document::dom::Element<'_>> =
                elements.get(&parent.id().get()).copied();
            if parent_element.is_none() && !parent.is_root() {
                return Err(XmlQueryError::Invalid);
            }
            match node.node_type() {
                roxmltree::NodeType::Element => {
                    let name = node.tag_name();
                    let element = target.create_element(sxd_document::QName::with_namespace_uri(
                        name.namespace(),
                        name.name(),
                    ));
                    for namespace in node.namespaces() {
                        if let Some(prefix) = namespace.name() {
                            element.register_prefix(prefix, namespace.uri());
                        } else {
                            element.set_default_namespace_uri(Some(namespace.uri()));
                        }
                    }
                    for attribute in node.attributes() {
                        element.set_attribute_value(
                            sxd_document::QName::with_namespace_uri(
                                attribute.namespace(),
                                attribute.name(),
                            ),
                            attribute.value(),
                        );
                    }
                    if let Some(parent) = parent_element {
                        parent.append_child(element);
                    } else {
                        target.root().append_child(element);
                    }
                    elements.insert(node.id().get(), element);
                }
                roxmltree::NodeType::Text => {
                    if let Some(parent) = parent_element {
                        parent.append_child(
                            target.create_text(node.text().ok_or(XmlQueryError::Invalid)?),
                        );
                    }
                }
                roxmltree::NodeType::Comment => {
                    let comment = target.create_comment(node.text().ok_or(XmlQueryError::Invalid)?);
                    if let Some(parent) = parent_element {
                        parent.append_child(comment);
                    } else {
                        target.root().append_child(comment);
                    }
                }
                roxmltree::NodeType::PI => {
                    let pi = node.pi().ok_or(XmlQueryError::Invalid)?;
                    let instruction = target.create_processing_instruction(pi.target, pi.value);
                    if let Some(parent) = parent_element {
                        parent.append_child(instruction);
                    } else {
                        target.root().append_child(instruction);
                    }
                }
                roxmltree::NodeType::Root => {}
            }
        }
        Ok(Self(package))
    }

    /// Borrow the document tree for XPath evaluation.
    pub(super) fn document(&self) -> sxd_document::dom::Document<'_> {
        self.0.as_document()
    }
}
