use super::admission::{self, MAX_DEPTH, MAX_NODES, MAX_OUTPUT_BYTES, MAX_XML_BYTES};
use super::host::Host;
use super::xml_query_error::XmlQueryError;
use super::xpath_document::XpathDocument;
use std::collections::BTreeMap;

/// Select XML nodes with a restricted XPath 1.0 path and return their string values.
/// Results follow document order.
///
/// Microsoft specifies XPath selection, invalid-XML errors and dynamic arrays:
/// <https://support.microsoft.com/en-us/excel/functions/filterxml-function>.
/// Its VBA reference limits XPath to 1,024 characters:
/// <https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.filterxml>.
/// XML/XPath semantics come from the MIT-licensed SXD libraries; dependency
/// revisions and notices are recorded in LICENSES/SXD.md. No upstream source,
/// examples, or tests were copied or adapted into this implementation.
///
/// XML is limited to 256 KiB and a combined count of 1,024 XML nodes, attributes
/// and in-scope namespace bindings. Depth is at most 64, counting the document
/// root and current node. XPath supports at most 32 slash-delimited segments
/// (including empty segments) with eight simple predicates per segment.
/// The backend rejects arbitrary functions, unions, predicates that contain paths
/// or nested predicates, and namespace prefixes bound to different URIs.
/// Estimated backend work is capped at 64 million units independently of caller
/// quotas. Before parsing, the backend also limits literal '=' bytes to 1,024
/// and charges for the work needed to check duplicate attributes;
/// these conservative limits include '=' inside text and quoted values.
/// The backend refuses DTDs before parsing. Output is limited to 1 MiB total,
/// with Excel's limit of 32,767 UTF-16 units per selected string.
/// Default namespaces follow XPath 1.0 semantics: unprefixed tests select only
/// unnamespaced elements; use local-name()/namespace-uri() predicates instead.
/// The caller converts the selected strings using its spreadsheet locale.
pub fn select<H: Host>(
    xml: &str,
    xpath: &str,
    host: &mut H,
) -> Result<Vec<String>, XmlQueryError<H::Error>> {
    if xml.len() > MAX_XML_BYTES {
        return Err(XmlQueryError::ResourceLimit);
    }
    let steps = admission::path(xpath)?;
    // Reject DTDs before invoking the parser. This conservative literal check
    // also refuses harmless DOCTYPE text inside comments and CDATA.
    if xml.contains("<!DOCTYPE") {
        return Err(XmlQueryError::DocumentType);
    }
    // roxmltree checks duplicate attributes pairwise. Bound its work before
    // parsing using '=' as a conservative upper count of attributes, including
    // occurrences in text and quoted values. This deliberately refuses those
    // literal-heavy inputs rather than entering an unmetered quadratic parse.
    let equals = xml.bytes().filter(|&byte| byte == b'=').count();
    let parse_work = (xml.len() as u64).saturating_mul(16 + (equals as u64).saturating_mul(4));
    if equals > MAX_NODES || parse_work > 64_000_000 {
        return Err(XmlQueryError::ResourceLimit);
    }
    host.text(xml.len()).map_err(XmlQueryError::Host)?;
    // Parsing first needs space for tokens and attributes before their exact
    // count is known. The byte bound also bounds their possible number.
    host.values(xml.len()).map_err(XmlQueryError::Host)?;
    host.work(parse_work).map_err(XmlQueryError::Host)?;
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES as u32,
            entity_resolver: None,
        },
    )
    .map_err(|error| match error {
        roxmltree::Error::NodesLimitReached => XmlQueryError::ResourceLimit,
        roxmltree::Error::DtdDetected => XmlQueryError::DocumentType,
        _ => XmlQueryError::Invalid,
    })?;
    let mut count = 0usize;
    let mut string_work = xml.len();
    let mut namespaces = BTreeMap::new();
    namespaces.insert("xml", "http://www.w3.org/XML/1998/namespace");
    for node in document.descendants() {
        count = count.saturating_add(
            1usize
                .saturating_add(node.attributes().len())
                .saturating_add(node.namespaces().len()),
        );
        let depth = node.ancestors().count();
        if count > MAX_NODES || depth > MAX_DEPTH {
            return Err(XmlQueryError::ResourceLimit);
        }
        // Each element string-value can revisit its descendant text. Sum
        // those visits now instead of assuming each text byte is visited once.
        if node.is_text() {
            string_work =
                string_work.saturating_add(node.text().map_or(0, str::len).saturating_mul(depth));
        }
        let name = node.tag_name();
        string_work = string_work
            .saturating_add(name.name().len())
            .saturating_add(name.namespace().map_or(0, str::len));
        for attribute in node.attributes() {
            string_work = string_work
                .saturating_add(attribute.name().len())
                .saturating_add(attribute.value().len())
                .saturating_add(attribute.namespace().map_or(0, str::len));
        }
        for namespace in node.namespaces() {
            if let Some(prefix) = namespace.name()
                && let Some(previous) = namespaces.insert(prefix, namespace.uri())
                && previous != namespace.uri()
            {
                return Err(XmlQueryError::NamespaceRebinding);
            }
        }
    }
    admission::prefixes(xpath, &namespaces)?;
    // Each step can scan every node for every candidate. Simple predicates
    // inspect only the current node/attributes/text, at most eight per step.
    // Include expanded names, attributes and repeated ancestor string-values
    // for worst-case comparisons; every candidate may recur in every context.
    // Charge a conservative quadratic estimate including backend ordering;
    // the admitted grammar never evaluates another path inside a predicate.
    let work = (steps as u64)
        .saturating_mul(count as u64)
        .saturating_mul(count.saturating_add(string_work) as u64)
        .saturating_mul(32);
    if work > 64_000_000 {
        return Err(XmlQueryError::ResourceLimit);
    }
    host.work(work).map_err(XmlQueryError::Host)?;
    host.values(count).map_err(XmlQueryError::Host)?;
    // An element's XPath string value cannot exceed the XML input size. Admit
    // that bound before SXD materializes any string, then check exact output.
    host.text(xml.len()).map_err(XmlQueryError::Host)?;
    let package = XpathDocument::new(&document)?;
    let expression = sxd_xpath::Factory::new()
        .build(xpath)
        .map_err(|_| XmlQueryError::Invalid)?
        .ok_or(XmlQueryError::Invalid)?;
    let mut context = sxd_xpath::Context::new();
    for (prefix, uri) in namespaces {
        context.set_namespace(prefix, uri);
    }
    let value = expression
        .evaluate(&context, package.document().root())
        .map_err(|_| XmlQueryError::Invalid)?;
    let sxd_xpath::Value::Nodeset(nodes) = value else {
        return Err(XmlQueryError::UnsupportedXPath);
    };
    if nodes.size() == 0 {
        return Err(XmlQueryError::Invalid);
    }
    host.values(nodes.size()).map_err(XmlQueryError::Host)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(nodes.size())
        .map_err(|_| XmlQueryError::Allocation)?;
    let mut total = 0usize;
    for node in nodes.document_order() {
        let value = node.string_value();
        if value.encode_utf16().count() > 32_767 {
            return Err(XmlQueryError::TextLimit);
        }
        total = total.saturating_add(value.len());
        if total > MAX_OUTPUT_BYTES {
            return Err(XmlQueryError::ResourceLimit);
        }
        host.text(value.len()).map_err(XmlQueryError::Host)?;
        // SAFETY: nodes.size() slots were reserved locally; iteration visits
        // exactly that many unique selected nodes in document order.
        values.push(value);
    }
    Ok(values)
}
