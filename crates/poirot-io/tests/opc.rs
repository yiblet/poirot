use poirot_io::{
    IoError,
    opc::{OpcError, PackageInventory, resolve_target},
    read_xlsx_bytes,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{ZipWriter, write::SimpleFileOptions};
fn inventory() -> PackageInventory {
    let mut parts = BTreeMap::new();
    for (path, xml) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="documents/book.xml"/></Relationships>"#,
        ),
        (
            "documents/_rels/book.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="cells" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/cells.xml"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../formats/custom.xml"/><Relationship Id="chart" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="../data/chart.xml"/></Relationships>"#,
        ),
        (
            "documents/book.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:links="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Chart" sheetId="7" links:id="chart"/><sheet name="Values" sheetId="42" links:id="cells"/></sheets><definedNames><definedName name="Same" localSheetId="0">Chart!$A$1</definedName><definedName name="Same" localSheetId="1">Values!$A$1</definedName></definedNames></workbook>"#,
        ),
        (
            "data/cells.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" s="1"><v>17</v></c></row></sheetData></worksheet>"#,
        ),
        (
            "data/chart.xml",
            r#"<chartsheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#,
        ),
        (
            "formats/custom.xml",
            r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="0.000"/></numFmts><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="164"/></cellXfs></styleSheet>"#,
        ),
    ] {
        parts.insert(path.into(), xml.as_bytes().to_vec());
    }
    PackageInventory { parts }
}
fn bytes(p: PackageInventory) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in p.parts {
        zip.start_file(path, SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
#[test]
fn relocated_parts_alternate_prefix_and_skipped_scope_execute_correctly() {
    let p = inventory();
    let plan = p.plan().unwrap();
    assert_eq!(plan.workbook_part, "documents/book.xml");
    assert_eq!(plan.sheets[1].part, "data/cells.xml");
    let report = read_xlsx_bytes(&bytes(p)).unwrap();
    assert_eq!(report.workbook.sheets().len(), 1);
    let sheet = &report.workbook.sheets()[0];
    assert_eq!(sheet.name(), "Values");
    let cell = sheet.get("A1".parse().unwrap()).unwrap();
    assert_eq!(cell.value, poirot_model::CellValue::Number(17.0));
    assert_eq!(cell.style.number_format.as_deref(), Some("0.000"));
    assert_eq!(report.workbook.named_ranges.len(), 1);
    assert_eq!(report.workbook.named_ranges[0].sheet_scope, Some(0));
    assert!(
        report
            .workbook
            .file_metadata
            .as_ref()
            .unwrap()
            .import_warnings
            .iter()
            .any(|w| w.detail.contains("scoped to omitted sheet"))
    );
}
#[test]
fn invalid_target_resolution_never_repairs_escape_or_uri_syntax() {
    assert_eq!(
        resolve_target("documents/book.xml", "../data/cells.xml").unwrap(),
        "data/cells.xml"
    );
    for target in [
        "../../escape.xml",
        "https://host/sheet.xml",
        "a\\b",
        "a%2fb",
        "a#fragment",
        "a//b",
    ] {
        assert!(matches!(
            resolve_target("documents/book.xml", target),
            Err(OpcError::InvalidTarget { .. })
        ));
    }
}
#[test]
fn dangling_duplicate_wrong_namespace_and_external_required_relationships_fail_typed() {
    let mut p = inventory();
    p.parts.remove("data/cells.xml");
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::MissingPart(_)))
    ));
    let mut p = inventory();
    let rels = p.parts.get_mut("documents/_rels/book.xml.rels").unwrap();
    *rels = String::from_utf8(rels.clone())
        .unwrap()
        .replace("Id=\"chart\"", "Id=\"cells\"")
        .into_bytes();
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::DuplicateIdentity(_)))
    ));
    let mut p = inventory();
    let workbook = p.parts.get_mut("documents/book.xml").unwrap();
    *workbook = String::from_utf8(workbook.clone())
        .unwrap()
        .replace(
            "xmlns:links=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"",
            "xmlns:links=\"urn:wrong\"",
        )
        .into_bytes();
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::SheetMismatch(_)))
    ));
    let mut p = inventory();
    let root = p.parts.get_mut("_rels/.rels").unwrap();
    *root = String::from_utf8(root.clone())
        .unwrap()
        .replace("Target=", "TargetMode=\"External\" Target=")
        .into_bytes();
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::ExternalRelationship(_)))
    ));
}

#[test]
fn invalid_local_scope_is_an_error_instead_of_a_global_or_neighbor_name() {
    let mut p = inventory();
    let workbook = p.parts.get_mut("documents/book.xml").unwrap();
    *workbook = String::from_utf8(workbook.clone())
        .unwrap()
        .replace("localSheetId=\"1\"", "localSheetId=\"2\"")
        .into_bytes();
    assert!(matches!(
        read_xlsx_bytes(&bytes(p)),
        Err(IoError::Opc(OpcError::SheetMismatch(_)))
    ));
}

#[test]
fn missing_sheet_relationship_and_wrong_relationship_type_fail_before_backend() {
    let mut p = inventory();
    let workbook = p.parts.get_mut("documents/book.xml").unwrap();
    *workbook = String::from_utf8(workbook.clone())
        .unwrap()
        .replace("links:id=\"cells\"", "links:id=\"absent\"")
        .into_bytes();
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::InvalidRelationship(_)))
    ));
    let mut p = inventory();
    let rels = p.parts.get_mut("documents/_rels/book.xml.rels").unwrap();
    *rels = String::from_utf8(rels.clone())
        .unwrap()
        .replace("relationships/worksheet", "relationships/styles")
        .into_bytes();
    assert!(matches!(
        p.plan(),
        Err(IoError::Opc(OpcError::InvalidRelationship(_)))
    ));
}

#[test]
fn external_link_policy_is_inspectable_and_executed_by_byte_adapter() {
    let mut p = inventory();
    let rels = p.parts.get_mut("documents/_rels/book.xml.rels").unwrap();
    *rels = String::from_utf8(rels.clone()).unwrap().replace("</Relationships>", "<Relationship Id=\"link\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink\" TargetMode=\"External\" Target=\"https://example.invalid/book.xlsx\"/></Relationships>").into_bytes();
    assert!(p.plan().unwrap().warnings.iter().any(|w| w.kind
        == poirot_model::ImportDiagnosticKind::ExternalLink
        && w.detail.contains("external relationship")));
    let policy = poirot_io::opc::OpcImportPolicy {
        external_relationships: poirot_io::opc::ExternalRelationshipPolicy::Reject,
    };
    assert!(matches!(
        p.plan_with_policy(policy),
        Err(IoError::Opc(OpcError::ExternalRelationship(_)))
    ));
    assert!(matches!(
        poirot_io::read_xlsx_bytes_with_policy(
            &bytes(p),
            poirot_io::ReadOptions::default(),
            policy
        ),
        Err(IoError::Opc(OpcError::ExternalRelationship(_)))
    ));
}

#[test]
fn legal_percent_encoded_parts_use_raw_zip_names_through_backend() {
    let p = inventory();
    let rename = |s: String| {
        s.replace("book.xml", "book%20name.xml")
            .replace("cells.xml", "cells%23%E2%82%AC.xml")
            .replace("custom.xml", "custom%25.xml")
    };
    let p = PackageInventory {
        parts: p
            .parts
            .into_iter()
            .map(|(path, xml)| {
                (
                    rename(path),
                    rename(String::from_utf8(xml).unwrap()).into_bytes(),
                )
            })
            .collect(),
    };
    let plan = p.plan().unwrap();
    assert_eq!(plan.workbook_part, "documents/book%20name.xml");
    assert_eq!(plan.sheets[1].part, "data/cells%23%E2%82%AC.xml");
    let report = read_xlsx_bytes(&bytes(p)).unwrap();
    let cell = report.workbook.sheets()[0]
        .get("A1".parse().unwrap())
        .unwrap();
    assert_eq!(cell.value, poirot_model::CellValue::Number(17.0));
    assert_eq!(cell.style.number_format.as_deref(), Some("0.000"));
    assert_eq!(report.workbook.named_ranges[0].sheet_scope, Some(0));
}

#[test]
fn encoded_uri_boundaries_are_checked_without_decoding_or_fallback() {
    for target in [
        "../data/a%20b.xml",
        "../data/a%23b.xml",
        "../data/%E2%82%AC.xml",
        "../data/a%25b.xml",
        "../data/a:b.xml",
    ] {
        assert_eq!(
            resolve_target("documents/book.xml", target).unwrap(),
            target.strip_prefix("../").unwrap()
        );
    }
    for target in [
        "a%",
        "a%2",
        "a%GG",
        "%2e%2e/escape.xml",
        "a%2Fb.xml",
        "a%5cb.xml",
        "a%41.xml",
        "../../escape%20name.xml",
        "//host/path.xml",
        "a//b.xml",
        "a/.../b.xml",
        "a/b./c.xml",
    ] {
        assert!(
            matches!(
                resolve_target("documents/book.xml", target),
                Err(OpcError::InvalidTarget { .. })
            ),
            "{target}"
        );
    }
    let mut p = inventory();
    let rels = p.parts.get_mut("documents/_rels/book.xml.rels").unwrap();
    *rels = String::from_utf8(rels.clone())
        .unwrap()
        .replace("cells.xml", "cells%20name.xml")
        .into_bytes();
    let cell_xml = p.parts.remove("data/cells.xml").unwrap();
    p.parts.insert("data/cells name.xml".into(), cell_xml);
    assert!(
        matches!(p.plan(), Err(IoError::Opc(OpcError::MissingPart(path))) if path == "data/cells%20name.xml")
    );
}

#[test]
fn identifier_plan_uses_shared_policy_and_preserves_raw_names() {
    let mut package = inventory();
    let xml = String::from_utf8(package.parts["documents/book.xml"].clone())
        .unwrap()
        .replace("name=\"Chart\"", "name=\"Ä\"")
        .replace("name=\"Values\"", "name=\"ä\"");
    package
        .parts
        .insert("documents/book.xml".into(), xml.into_bytes());
    assert!(matches!(
        package.plan(),
        Err(IoError::Opc(OpcError::DuplicateIdentity(_)))
    ));
    let xml = String::from_utf8(package.parts["documents/book.xml"].clone())
        .unwrap()
        .replace("name=\"Ä\"", "name=\"Σ\"")
        .replace("name=\"ä\"", "name=\"σ\"");
    package
        .parts
        .insert("documents/book.xml".into(), xml.into_bytes());
    let plan = package.plan().unwrap();
    assert_eq!(plan.sheets[0].name, "Σ");
    assert_eq!(plan.sheets[1].name, "σ");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.detail.contains("identifier comparison unsupported"))
    );
}
