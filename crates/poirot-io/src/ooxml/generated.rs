//! Normalize backend-authored package defaults before the canonical projection.
use super::package::Package;
use super::xml::{attr, events, write_event, xml};
use crate::IoError;
use quick_xml::{
    Writer,
    events::{BytesStart, Event},
};

/// Backend default theme part, replaced only by a workbook's own theme.
const DEFAULT_THEME: &str = "xl/theme/theme1.xml";

/// Remove backend defaults that are not part of the modeled workbook.
///
/// rust_xlsxwriter always authors an Office theme and wall-clock creation
/// properties. Exports carry a theme only when the source workbook has one, and
/// equal workbooks produce identical packages.
pub(super) fn normalize(package: &mut Package) -> Result<(), IoError> {
    if package.remove(DEFAULT_THEME).is_some() {
        let theme = format!("/{DEFAULT_THEME}");
        filter(package, "[Content_Types].xml", |e| {
            Ok(e.local_name().as_ref() == "Override"
                && attr(e, "PartName")?.as_deref() == Some(theme.as_str()))
        })?;
        filter(package, "xl/_rels/workbook.xml.rels", |e| {
            Ok(e.local_name().as_ref() == "Relationship"
                && attr(e, "Type")?.is_some_and(|kind| kind.ends_with("/relationships/theme")))
        })?;
    }
    filter(package, "docProps/core.xml", |e| {
        Ok(matches!(e.local_name().as_ref(), "created" | "modified"))
    })
}

/// Rewrite one part without the elements selected by `skip`; absent parts are unchanged.
fn filter(
    package: &mut Package,
    path: &str,
    skip: impl Fn(&BytesStart<'_>) -> Result<bool, IoError>,
) -> Result<(), IoError> {
    let Some(source) = package.get(path) else {
        return Ok(());
    };
    let mut reader = events(source);
    let mut writer = Writer::new(Vec::with_capacity(source.len()));
    loop {
        let event = reader.read_event().map_err(xml)?;
        match &event {
            Event::Start(e) if skip(e)? => {
                reader.read_to_end(e.name()).map_err(xml)?;
                continue;
            }
            Event::Eof => break,
            _ => {}
        }
        write_event(&mut writer, event.into_owned())?;
    }
    package.insert(path.into(), writer.into_inner());
    Ok(())
}
