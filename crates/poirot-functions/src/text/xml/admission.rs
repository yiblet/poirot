use super::xml_query_error::XmlQueryError;

/// Maximum backend input size, including markup and attribute values.
pub(super) const MAX_XML_BYTES: usize = 262_144;
/// Maximum XML nodes, attributes and in-scope namespace bindings counted together.
pub(super) const MAX_NODES: usize = 1024;
/// Maximum XML depth, counting both the current node and the document root.
pub(super) const MAX_DEPTH: usize = 64;
/// Maximum result bytes across all selected nodes.
pub(super) const MAX_OUTPUT_BYTES: usize = 1_048_576;

/// Check that XPath uses one location path with a limited number of simple predicates.
/// Predicates cannot contain paths, nested predicates or arbitrary function calls.
/// These restrictions prevent repeated XPath evaluation from multiplying traversal work.
pub(super) fn path<E>(xpath: &str) -> Result<usize, XmlQueryError<E>> {
    if xpath.encode_utf16().count() > 1024 {
        return Err(XmlQueryError::XPathLength);
    }
    if xpath.trim().is_empty() {
        return Err(XmlQueryError::Invalid);
    }
    let mut quote = None;
    let mut predicate = false;
    let mut segment_start = 0;
    let mut predicate_start = 0;
    let mut steps = 1usize;
    let mut predicates = 0usize;
    for (index, character) in xpath.char_indices() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '[' if predicate => return Err(XmlQueryError::UnsupportedXPath),
            '[' => {
                node_test(
                    xpath
                        .get(segment_start..index)
                        .ok_or(XmlQueryError::Invalid)?,
                )?;
                predicate = true;
                predicate_start = index + 1;
                predicates += 1;
                if predicates > 8 {
                    return Err(XmlQueryError::ResourceLimit);
                }
            }
            ']' if !predicate => return Err(XmlQueryError::Invalid),
            ']' => {
                simple_predicate(
                    xpath
                        .get(predicate_start..index)
                        .ok_or(XmlQueryError::Invalid)?,
                )?;
                predicate = false;
                segment_start = index + 1;
            }
            '/' if !predicate => {
                node_test(
                    xpath
                        .get(segment_start..index)
                        .ok_or(XmlQueryError::Invalid)?,
                )?;
                segment_start = index + 1;
                predicates = 0;
                steps += 1;
                if steps > 32 {
                    return Err(XmlQueryError::ResourceLimit);
                }
            }
            _ => {}
        }
    }
    if quote.is_some() || predicate {
        return Err(XmlQueryError::Invalid);
    }
    node_test(xpath.get(segment_start..).ok_or(XmlQueryError::Invalid)?)?;
    Ok(steps)
}

/// Allow only node tests and XPath axes; let the XPath compiler validate names.
fn node_test<E>(test: &str) -> Result<(), XmlQueryError<E>> {
    let test = test.trim();
    // Empty segments describe /, //, or text after a closing predicate; XPath's
    // compiler validates their placement. No recursive expression is admitted.
    if test.is_empty() || matches!(test, "." | ".." | "text()" | "node()" | "comment()") {
        return Ok(());
    }
    let test = if let Some((axis, name)) = test.split_once("::") {
        if !matches!(
            axis,
            "child"
                | "descendant"
                | "descendant-or-self"
                | "parent"
                | "ancestor"
                | "ancestor-or-self"
                | "self"
                | "following"
                | "preceding"
                | "following-sibling"
                | "preceding-sibling"
                | "attribute"
        ) {
            return Err(XmlQueryError::UnsupportedXPath);
        }
        name
    } else {
        test.strip_prefix('@').unwrap_or(test)
    };
    if matches!(test, "text()" | "node()" | "comment()") {
        return Ok(());
    }
    if test.chars().all(name_character) {
        Ok(())
    } else {
        Err(XmlQueryError::UnsupportedXPath)
    }
}

/// Permit positional, attribute, and current-node comparisons in predicates.
fn simple_predicate<E>(predicate: &str) -> Result<(), XmlQueryError<E>> {
    let predicate = predicate.trim();
    if predicate.chars().all(|c| c.is_ascii_digit()) || predicate == "last()" {
        return Ok(());
    }
    let comparison = predicate
        .char_indices()
        .find(|(_, c)| matches!(c, '=' | '<' | '>' | '!'));
    let Some((index, _)) = comparison else {
        return if let Some(name) = predicate.strip_prefix('@') {
            node_test(name)
        } else {
            Err(XmlQueryError::UnsupportedXPath)
        };
    };
    let left = predicate.get(..index).ok_or(XmlQueryError::Invalid)?.trim();
    if !matches!(
        left,
        "." | "text()" | "position()" | "local-name()" | "namespace-uri()"
    ) {
        let Some(name) = left.strip_prefix('@') else {
            return Err(XmlQueryError::UnsupportedXPath);
        };
        node_test(name)?;
    }
    let right = predicate
        .get(index..)
        .ok_or(XmlQueryError::Invalid)?
        .trim_start_matches(['=', '<', '>', '!'])
        .trim();
    let number = right.strip_prefix('-').unwrap_or(right);
    if right == "last()"
        || (number.chars().any(|c| c.is_ascii_digit())
            && number.chars().all(|c| c.is_ascii_digit() || c == '.')
            && number.chars().filter(|&c| c == '.').count() <= 1)
    {
        return Ok(());
    }
    let mut characters = right.chars();
    if let Some(delimiter @ ('\'' | '"')) = characters.next() {
        let rest = characters.as_str();
        if let Some(content) = rest.strip_suffix(delimiter)
            && !content.contains(delimiter)
        {
            return Ok(());
        }
    }
    Err(XmlQueryError::UnsupportedXPath)
}

/// Reject undefined prefixes before SXD looks them up, which could cause a panic.
/// Scan names outside string literals and distinguish the axis separator `::`.
pub(super) fn prefixes<E>(
    xpath: &str,
    namespaces: &std::collections::BTreeMap<&str, &str>,
) -> Result<(), XmlQueryError<E>> {
    let mut quote = None;
    let mut start = 0usize;
    for (index, character) in xpath
        .char_indices()
        .chain(std::iter::once((xpath.len(), ' ')))
    {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
            start = index + character.len_utf8();
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
        }
        if name_character(character) {
            continue;
        }
        let token = xpath.get(start..index).ok_or(XmlQueryError::Invalid)?;
        let name = token.rsplit("::").next().unwrap_or(token);
        if let Some((prefix, _)) = name.split_once(':')
            && !namespaces.contains_key(prefix)
        {
            return Err(XmlQueryError::Invalid);
        }
        start = index + character.len_utf8();
    }
    Ok(())
}

/// Allow possible XML name characters while excluding XPath punctuation.
/// The XML/XPath parsers then check the exact Unicode name rules.
fn name_character(character: char) -> bool {
    character.is_alphanumeric()
        || matches!(character, '_' | '-' | '.' | ':' | '*')
        || (!character.is_ascii() && !character.is_whitespace())
}
