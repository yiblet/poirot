use poirot_io::{
    IoError, PreflightError, ReadOptions, ResourceBudget, read_workbook_bytes,
    read_workbook_bytes_with_options,
};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

fn record(id: u16, data: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(id.to_le_bytes());
    b.extend((data.len() as u16).to_le_bytes());
    b.extend(data);
    b
}
fn compound(stream: &[u8]) -> Vec<u8> {
    const END: u32 = 0xffff_fffe;
    let mut b = vec![0; 512 * 11];
    b[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    b[26..28].copy_from_slice(&3u16.to_le_bytes());
    b[28..30].copy_from_slice(&0xfffeu16.to_le_bytes());
    b[30..32].copy_from_slice(&9u16.to_le_bytes());
    b[32..34].copy_from_slice(&6u16.to_le_bytes());
    b[44..48].copy_from_slice(&1u32.to_le_bytes());
    b[48..52].copy_from_slice(&1u32.to_le_bytes());
    b[56..60].copy_from_slice(&4096u32.to_le_bytes());
    b[60..64].copy_from_slice(&END.to_le_bytes());
    b[68..72].copy_from_slice(&END.to_le_bytes());
    b[76..512].fill(255);
    b[76..80].copy_from_slice(&0u32.to_le_bytes());
    b[512..1024].fill(255);
    for (id, next) in [
        (0, 0xffff_fffdu32),
        (1, END),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 8),
        (8, 9),
        (9, END),
    ] {
        b[512 + id * 4..516 + id * 4].copy_from_slice(&next.to_le_bytes());
    }
    b[1024 + 66] = 5;
    b[1024 + 116..1024 + 120].copy_from_slice(&END.to_le_bytes());
    let d = 1024 + 128;
    for (i, c) in "Workbook".encode_utf16().enumerate() {
        b[d + i * 2..d + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    b[d + 64..d + 66].copy_from_slice(&18u16.to_le_bytes());
    b[d + 66] = 2;
    b[d + 116..d + 120].copy_from_slice(&2u32.to_le_bytes());
    b[d + 120..d + 124].copy_from_slice(&4096u32.to_le_bytes());
    b[1536..1536 + stream.len()].copy_from_slice(stream);
    b
}
#[test]
fn tiny_budget_refuses_invalid_cfb_before_backend_decode() {
    // No FAT or directories: this cannot be decoded as a compound file. The typed
    // resource result proves the preflight ran first and suppressed the backend.
    let mut b = vec![0; 512];
    b[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    b[30..32].copy_from_slice(&9u16.to_le_bytes());
    b[32..34].copy_from_slice(&6u16.to_le_bytes());
    b[44..48].copy_from_slice(&1u32.to_le_bytes());
    let budget = ResourceBudget {
        max_metadata_entries: 0,
        ..ResourceBudget::default()
    };
    assert!(matches!(
        read_workbook_bytes_with_options::<poirot::model::Workbook>(
            &b,
            poirot_io::ReadOptions {
                xls_budget: budget,
                ..ReadOptions::default()
            }
        ),
        Err(IoError::BiffPreflight(PreflightError::Resource {
            resource: "CFB FAT capacity",
            requested: 1,
            limit: 0
        }))
    ));
}
#[test]
fn malformed_chain_is_typed_and_does_not_panic() {
    let mut b = compound(&[]);
    b[512 + 9 * 4..516 + 9 * 4].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        read_workbook_bytes::<poirot::model::Workbook>(&b),
        Err(IoError::BiffPreflight(PreflightError::Malformed(
            "cyclic sector chain"
        )))
    ));
}
#[test]
fn huge_extent_does_not_allocate_a_dense_rectangle() {
    const CHILD: &str = "ROI_BIFF_HAZARD_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut bound = vec![0; 9];
        bound[..4].copy_from_slice(&25u32.to_le_bytes());
        bound[6] = 1;
        bound[8] = b'S';
        let mut stream = record(0x809, &[0, 6, 5, 0]);
        stream.extend(record(0x85, &bound));
        stream.extend(record(0xa, &[]));
        stream.extend(record(0x809, &[0, 6, 16, 0]));
        let mut dimensions = vec![0; 14];
        dimensions[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        dimensions[10..12].copy_from_slice(&256u16.to_le_bytes());
        stream.extend(record(0x200, &dimensions));
        stream.extend(record(0xa, &[]));
        let actual = read_workbook_bytes::<poirot::model::Workbook>(&compound(&stream));
        // rxls ignores Dimensions reservations and stores actual cells only.
        let workbook = actual.unwrap();
        assert_eq!(workbook.sheets().len(), 1);
        assert!(workbook.sheet(0).unwrap().cells().is_empty());
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "huge_extent_does_not_allocate_a_dense_rectangle",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("sparse BIFF decoding did not terminate within five seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn ordinary_unicode_and_cached_string_fixtures_keep_default_imports_and_warnings() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "crates/poirot-io/tests/fixtures/poi-SimpleMultiCell.xls",
        "crates/poirot-io/tests/fixtures/generated-xls/unicode-formula.xls",
        "crates/poirot-io/tests/fixtures/generated-xls/unicode-name.xls",
        "crates/poirot-io/tests/fixtures/generated-xls/long-cached-string-continuation.xls",
        "crates/poirot-io/tests/fixtures/generated-xls/relative-area.xls",
    ] {
        let bytes = std::fs::read(root.join(path)).unwrap();
        let default = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
        let explicit = read_workbook_bytes_with_options::<poirot::model::Workbook>(
            &bytes,
            poirot_io::ReadOptions {
                xls_budget: ResourceBudget::default(),
                ..ReadOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            default.file_metadata.as_ref().unwrap().import_warnings,
            explicit.file_metadata.as_ref().unwrap().import_warnings,
            "{path}"
        );
        assert_eq!(default.sheets().len(), explicit.sheets().len(), "{path}");
        for (left, right) in default.sheets().iter().zip(explicit.sheets()) {
            assert_eq!(left.cells(), right.cells(), "{path}");
        }
        assert!(
            default
                .file_metadata
                .as_ref()
                .unwrap()
                .import_warnings
                .iter()
                .any(|w| w.detail.contains("XLS raw number formats"))
                && default
                    .file_metadata
                    .as_ref()
                    .unwrap()
                    .import_warnings
                    .iter()
                    .any(|w| w.detail.contains("unsupported BIFF token families")),
            "{path}"
        );
    }
}
