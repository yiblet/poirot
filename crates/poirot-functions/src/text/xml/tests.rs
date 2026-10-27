use super::{Host, XmlQueryError, select};
use std::convert::Infallible;

/// Admit small independent XML fixtures without workbook state.
struct Unrestricted;

impl Host for Unrestricted {
    type Error = Infallible;

    fn work(&mut self, _: u64) -> Result<(), Infallible> {
        Ok(())
    }
    fn text(&mut self, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn values(&mut self, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
}

// Expectations are independently derived from the XPath 1.0 Recommendation,
// https://www.w3.org/TR/1999/REC-xpath-19991116/ (location paths/string values),
// and Microsoft's FILTERXML documentation linked beside select. These original
// fixture values are not copied from upstream tests or a spreadsheet engine.
#[test]
fn selects_repeated_elements_attributes_and_mixed_text_in_document_order() {
    let xml = "<r><item id='a'>one<b>two</b>three</item><item id='b'>four</item></r>";
    assert_eq!(
        select(xml, "//item", &mut Unrestricted).unwrap(),
        ["onetwothree", "four"]
    );
    assert_eq!(
        select(xml, "//item/@id", &mut Unrestricted).unwrap(),
        ["a", "b"]
    );
    assert_eq!(
        select(xml, "//item[2]/text()", &mut Unrestricted).unwrap(),
        ["four"]
    );
    assert_eq!(
        select(xml, "//item[@id='a']/b", &mut Unrestricted).unwrap(),
        ["two"]
    );
    assert_eq!(
        select(xml, "//item[last()]", &mut Unrestricted).unwrap(),
        ["four"]
    );
    assert_eq!(
        select(xml, "//item[position()=last()]", &mut Unrestricted).unwrap(),
        ["four"]
    );
}

#[test]
fn handles_namespaces_entities_unicode_and_xpath_axes() {
    let xml = "<r xmlns:p='urn:x'><p:item>é&amp;水</p:item><p:item>next</p:item></r>";
    assert_eq!(
        select(xml, "//p:item[1]/text()", &mut Unrestricted).unwrap(),
        ["é&水"]
    );
    assert_eq!(
        select(xml, "//p:item", &mut Unrestricted).unwrap(),
        ["é&水", "next"]
    );
    assert_eq!(
        select(xml, "//*[local-name()='item'][1]", &mut Unrestricted).unwrap(),
        ["é&水"]
    );
    assert_eq!(
        select(
            xml,
            "//p:item[1]/following-sibling::p:item",
            &mut Unrestricted
        )
        .unwrap(),
        ["next"]
    );
    let xml = "<r xmlns:ṕ='urn:x'><ṕ:í>yes</ṕ:í></r>";
    assert_eq!(select(xml, "//ṕ:í", &mut Unrestricted).unwrap(), ["yes"]);
    assert!(matches!(
        select("<r/>", "//q́:i", &mut Unrestricted),
        Err(XmlQueryError::Invalid)
    ));
    let xml = "<r xmlns='urn:x'><item>yes</item></r>";
    assert!(matches!(
        select(xml, "//item", &mut Unrestricted),
        Err(XmlQueryError::Invalid)
    ));
    assert_eq!(
        select(xml, "//*[local-name()='item']", &mut Unrestricted).unwrap(),
        ["yes"]
    );
}

#[test]
fn separates_invalid_inputs_unsupported_expressions_and_resource_bounds() {
    for (xml, xpath) in [
        ("<r>", "//r"),
        ("<r/>", "//missing"),
        ("<r/>", "//r["),
        ("<r/>", "//q:r"),
    ] {
        assert!(matches!(
            select(xml, xpath, &mut Unrestricted),
            Err(XmlQueryError::Invalid)
        ));
    }
    for xpath in [
        "count(//r)",
        "//r[child/item]",
        "//r[.='x' or .='y']",
        "//r | //x",
        "//r[//r[1]]",
    ] {
        assert!(
            matches!(
                select("<r/>", xpath, &mut Unrestricted),
                Err(XmlQueryError::UnsupportedXPath)
            ),
            "{xpath}"
        );
    }
    assert!(matches!(
        select(
            "<!DOCTYPE r SYSTEM 'file:///etc/passwd'><r/>",
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::DocumentType)
    ));
    assert!(matches!(
        select(
            "<!DOCTYPE r [<!ENTITY x 'xx'>]><r>&x;</r>",
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::DocumentType)
    ));
    assert!(matches!(
        select(
            "<r xmlns:p='a'><i xmlns:p='b'/></r>",
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::NamespaceRebinding)
    ));
    assert!(matches!(
        select(
            &format!("<r>{}</r>", "=".repeat(1025)),
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::ResourceLimit)
    ));
    assert!(matches!(
        select("<r/>", &"a".repeat(1025), &mut Unrestricted),
        Err(XmlQueryError::XPathLength)
    ));
    assert!(matches!(
        select(
            &format!("<r>{}</r>", "a".repeat(262_144)),
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::ResourceLimit)
    ));
    assert!(matches!(
        select(
            &format!("<r>{}</r>", "a".repeat(32_768)),
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::TextLimit)
    ));
    assert!(matches!(
        select(
            &format!("<r>{}</r>", "<i/>".repeat(1025)),
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::ResourceLimit)
    ));
    assert!(matches!(
        select(
            &format!("{}x{}", "<r>".repeat(65), "</r>".repeat(65)),
            "//r",
            &mut Unrestricted
        ),
        Err(XmlQueryError::ResourceLimit)
    ));
    assert!(matches!(
        select("<r/>", &format!("/{}", "r/".repeat(33)), &mut Unrestricted),
        Err(XmlQueryError::ResourceLimit)
    ));
    assert!(matches!(
        select(
            "<r/>",
            &format!("//r{}", "[1]".repeat(9)),
            &mut Unrestricted
        ),
        Err(XmlQueryError::ResourceLimit)
    ));
    // Selecting ancestors repeats the descendant string.
    let large = format!(
        "<r>{}{}{}</r>",
        "<i>".repeat(40),
        "a".repeat(30_000),
        "</i>".repeat(40)
    );
    assert!(matches!(
        select(&large, "//i", &mut Unrestricted),
        Err(XmlQueryError::ResourceLimit)
    ));
}

/// Refuse every backend operation with a typed caller error.
struct Refuse;

impl Host for Refuse {
    type Error = std::io::Error;

    fn work(&mut self, _: u64) -> Result<(), Self::Error> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }
    fn text(&mut self, _: usize) -> Result<(), Self::Error> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }
    fn values(&mut self, _: usize) -> Result<(), Self::Error> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }
}

#[test]
fn retains_caller_failure_and_error_source() {
    let error = select("<r/>", "//r", &mut Refuse).unwrap_err();
    assert!(
        matches!(&error, XmlQueryError::Host(source) if source.kind() == std::io::ErrorKind::PermissionDenied)
    );
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .is::<std::io::Error>()
    );
    fn send_sync<T: Send + Sync + 'static>() {}
    send_sync::<XmlQueryError<std::io::Error>>();
}
