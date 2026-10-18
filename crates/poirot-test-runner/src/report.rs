//! Terminal, JSONL and offline HTML views of the same typed observations.
use crate::{
    Error, invalid,
    model::*,
    runner::{Header, RunReport, Summary},
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Format {
    #[default]
    Text,
    Json,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
enum Record {
    Header { header: Header },
    Case { result: Box<CaseResult> },
    Removed { id: String },
    Summary { summary: Summary },
}

fn json(output: &mut impl Write, value: &Record) -> Result<(), Error> {
    serde_json::to_writer(&mut *output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

pub fn begin(
    output: &mut impl Write,
    format: Format,
    header: &Header,
    selected: usize,
) -> Result<(), Error> {
    match format {
        Format::Json => json(
            output,
            &Record::Header {
                header: header.clone(),
            },
        ),
        Format::Text => {
            writeln!(
                output,
                "Poirot tests · baseline {} · {} workers",
                header.baseline_path.display(),
                header.workers
            )?;
            writeln!(
                output,
                "Selected {selected} of {} discovered cases\n",
                header.inventory.len()
            )?;
            output.flush()?;
            Ok(())
        }
    }
}

fn label(case: &CaseResult) -> &'static str {
    match (case.observation.outcome, case.change) {
        (Outcome::HarnessError, _) => "HARNESS ERROR",
        (Outcome::Skipped, _) => "SKIP",
        _ if case.fails(false) => "FAIL",
        (Outcome::Passed, Change::Improvement) => "IMPROVED",
        (Outcome::Passed, _) => "PASS",
        (Outcome::Blocked, _) => "BLOCKED",
        _ => "GAP",
    }
}

/// Escape control characters so source text cannot inject terminal lines or ANSI commands.
pub fn single_line(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Bash/zsh ANSI-C quoting retains literal newlines without injecting terminal controls.
pub fn shell_quote(value: &str) -> String {
    let mut quoted = String::from("$'");
    for c in value.chars() {
        match c {
            '\'' => quoted.push_str("\\'"),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => {
                use std::fmt::Write;
                // Writing into a String cannot fail; avoid a panic-capable unwrap.
                let _ = write!(&mut quoted, "\\U{:08x}", u32::from(c));
            }
            _ => quoted.push(c),
        }
    }
    quoted.push('\'');
    quoted
}

pub fn detail(output: &mut impl Write, result: &CaseResult) -> Result<(), Error> {
    writeln!(
        output,
        "  {}  {}",
        label(result),
        single_line(&result.case.name)
    )?;
    writeln!(output, "    ID:        {}", single_line(&result.case.id))?;
    writeln!(
        output,
        "    Location:  {}",
        single_line(&result.case.location)
    )?;
    writeln!(
        output,
        "    Authority: {}",
        single_line(&result.case.authority)
    )?;
    if let Some(formula) = &result.case.formula {
        writeln!(output, "    Formula:   {}", single_line(formula))?;
    }
    match &result.baseline {
        Some(observation) => writeln!(
            output,
            "    Baseline:  {:?} / {}",
            observation.outcome,
            observation.category.as_deref().unwrap_or("none")
        )?,
        None => writeln!(output, "    Baseline:  no recorded observation")?,
    }
    writeln!(
        output,
        "    Current:   {:?} / {} · {:?}",
        result.observation.outcome,
        result.observation.category.as_deref().unwrap_or("none"),
        result.change
    )?;
    for line in result.reason.lines() {
        writeln!(output, "    {}", single_line(line))?;
    }
    let quoted = shell_quote(&result.case.id);
    writeln!(
        output,
        "    Rerun: cargo run -p poirot-test-runner -- run --case {quoted}"
    )?;
    Ok(())
}

pub fn suite(
    output: &mut impl Write,
    format: Format,
    cases: &[CaseResult],
    verbose: bool,
    quiet: bool,
) -> Result<(), Error> {
    if format == Format::Json {
        for result in cases {
            json(
                output,
                &Record::Case {
                    result: Box::new(result.clone()),
                },
            )?;
        }
        return Ok(());
    }
    let Some(first) = cases.first() else {
        return Ok(());
    };
    let failing = cases.iter().any(|case| case.fails(false));
    if !quiet || failing {
        let mut counts = BTreeMap::new();
        for case in cases {
            *counts.entry(case.observation.outcome).or_insert(0usize) += 1;
        }
        writeln!(
            output,
            "{} / {} / {}  {}",
            first.case.source,
            single_line(&first.case.group),
            single_line(&first.case.suite),
            counts
                .iter()
                .map(|(outcome, count)| format!("{count} {outcome:?}"))
                .collect::<Vec<_>>()
                .join(" · ")
        )?;
        let mut reasons = BTreeMap::new();
        for case in cases {
            if verbose || case.fails(false) || case.observation.outcome == Outcome::Skipped {
                detail(output, case)?;
            } else if case.observation.outcome != Outcome::Passed {
                let key = case.observation.category.as_deref().unwrap_or("unknown");
                let entry = reasons.entry(key).or_insert((0usize, case));
                entry.0 += 1;
            }
        }
        for (category, (count, example)) in reasons {
            writeln!(output, "  {count} {}: {category}", label(example))?;
            writeln!(
                output,
                "    Example: {}",
                single_line(example.reason.lines().next().unwrap_or_default())
            )?;
            writeln!(output, "    Case: {}", single_line(&example.case.id))?;
        }
        output.flush()?;
    }
    Ok(())
}

pub fn finish(output: &mut impl Write, format: Format, report: &RunReport) -> Result<(), Error> {
    if format == Format::Json {
        for id in &report.removed {
            json(output, &Record::Removed { id: id.clone() })?;
        }
        return json(
            output,
            &Record::Summary {
                summary: report.summary.clone(),
            },
        );
    }
    writeln!(output, "\nResults · {} selected", report.summary.selected)?;
    for (outcome, count) in &report.summary.outcomes {
        writeln!(output, "  {count:>6} {outcome:?}")?;
    }
    writeln!(output, "\nChanges since baseline")?;
    for (change, count) in &report.summary.changes {
        if *change != Change::Unchanged {
            writeln!(output, "  {count:>6} {change:?}")?;
        }
    }
    writeln!(
        output,
        "  {:>6} RemovedTest{}",
        report.removed.len(),
        if report.header.complete_inventory {
            ""
        } else {
            " (not assessed in a filtered/custom run)"
        }
    )?;
    for id in &report.removed {
        writeln!(output, "    {}", single_line(id))?;
    }
    writeln!(
        output,
        "\n{}",
        if report.summary.successful {
            "PASS"
        } else {
            "FAIL · review changes and diagnostics above"
        }
    )?;
    output.flush()?;
    Ok(())
}

pub fn read(path: &Path) -> Result<RunReport, Error> {
    let mut header = None;
    let mut cases = Vec::new();
    let mut removed = Vec::new();
    let mut summary = None;
    for line in std::io::BufReader::new(std::fs::File::open(path)?).lines() {
        if summary.is_some() {
            return Err(invalid("records after report summary"));
        }
        let record: Record = serde_json::from_str(&line?)?;
        match record {
            Record::Header { header: value }
                if header.is_none() && cases.is_empty() && removed.is_empty() =>
            {
                header = Some(value);
            }
            Record::Header { .. } => return Err(invalid("duplicate or misplaced report header")),
            Record::Case { result } if header.is_some() => cases.push(*result),
            Record::Removed { id } if header.is_some() => removed.push(id),
            Record::Summary { summary: value } if header.is_some() => summary = Some(value),
            _ => return Err(invalid("report must begin with a header")),
        }
    }
    let header = header.ok_or_else(|| invalid("missing report header"))?;
    let summary = summary.ok_or_else(|| {
        invalid("incomplete report: missing final summary; baseline cannot be updated")
    })?;
    if header.schema_version != 1 {
        return Err(invalid("unsupported report schema"));
    }
    let mut ids = BTreeSet::new();
    let inventory: BTreeMap<_, _> = header
        .inventory
        .iter()
        .map(|case| (&case.id, case))
        .collect();
    if inventory.len() != header.inventory.len() {
        return Err(invalid("duplicate report inventory ID"));
    }
    for result in &cases {
        if !ids.insert(result.case.id.clone())
            || !inventory.contains_key(&result.case.id)
            || !header.selection.matches(&result.case)
            || inventory.get(&result.case.id).copied() != Some(&result.case)
        {
            return Err(invalid(
                "report case is duplicate, outside selection or inconsistent with inventory",
            ));
        }
        if crate::baseline::compare(result.baseline.as_ref(), &result.observation) != result.change
        {
            return Err(invalid(
                "report comparison is inconsistent with its baseline observation",
            ));
        }
    }
    let expected: BTreeSet<_> = header
        .inventory
        .iter()
        .filter(|case| header.selection.matches(case))
        .map(|case| case.id.clone())
        .collect();
    if ids != expected
        || removed.iter().collect::<BTreeSet<_>>().len() != removed.len()
        || removed.iter().any(|id| inventory.contains_key(id))
        || (!header.complete_inventory && !removed.is_empty())
    {
        return Err(invalid(
            "report is missing selected cases or has invalid removals",
        ));
    }
    let mut report = RunReport {
        header,
        cases,
        removed,
        summary: Summary::default(),
    };
    report.summarize();
    if report.summary != summary {
        return Err(invalid("report summary does not match its case records"));
    }
    Ok(report)
}

/// Standalone viewer: all source content is data, rendered only with textContent.
pub fn html(report: &RunReport) -> Result<String, Error> {
    let data = serde_json::to_string(report)?
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(include_str!("report.html").replace("__REPORT_DATA__", &data))
}
