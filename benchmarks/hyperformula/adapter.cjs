// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 Poirot contributors
// Original benchmark adapter using HyperFormula's public API (see ../LICENSES.md).
// This adapter selects the GPLv3 distribution and runs in its own process/image.
'use strict';
const fs = require('node:fs');
const { HyperFormula } = require('hyperformula');
const version = HyperFormula.version;

function validate(actual, expected, name) {
  if (!Array.isArray(expected) || actual.length !== expected.length || !actual.length) {
    throw new Error(`${name}: output count mismatch`);
  }
  actual.forEach((value, i) => {
    const reference = expected[i];
    if (typeof value !== 'number' || !Number.isFinite(value) ||
        typeof reference !== 'number' || !Number.isFinite(reference) ||
        Math.abs(value - reference) > 1e-9 + 1e-10 * Math.abs(reference)) {
      throw new Error(`${name}: output ${i}: expected ${reference}, got ${JSON.stringify(value)}`);
    }
  });
}

function build(test, arrayArithmetic) {
  const sheets = Array.from({length: test.sheet_count || 1}, () => []);
  for (const cell of test.cells) {
    const rows = sheets[cell.sheet || 0];
    while (rows.length <= cell.row) rows.push([]);
    const row = rows[cell.row];
    while (row.length <= cell.col) row.push(null);
    row[cell.col] = typeof cell.input === 'object' ? "'" + cell.input.text : cell.input;
  }
  // Native construction eagerly calculates; included in build_calculate_read.
  return sheets.length === 1 ? HyperFormula.buildFromArray(sheets[0], { licenseKey: 'gpl-v3', useArrayArithmetic: arrayArithmetic }) :
    HyperFormula.buildFromSheets(Object.fromEntries(sheets.map((rows, i) => [`Sheet${i + 1}`, rows])), { licenseKey: 'gpl-v3', useArrayArithmetic: arrayArithmetic });
}

function read(model, outputs) {
  return outputs.map(({ row, col, sheet = 0 }) => model.getCellValue({ sheet, row, col }));
}

function validateSuite(suite) {
  if (suite.schema_version === 1 && suite.unchanged_read) throw new Error('unchanged_read requires schema2');
  for (const test of suite.cases) {
    const count = test.sheet_count === undefined ? 1 : test.sheet_count;
    if (!Number.isInteger(count) || count < 1 || (suite.schema_version === 1 && count !== 1)) throw new Error('multiple sheets require schema2');
    for (const field of ['cells', 'outputs', 'edits']) for (const item of test[field]) {
      const sheet = item.sheet === undefined ? 0 : item.sheet;
      if (![sheet, item.row, item.col].every(Number.isInteger) || sheet < 0 || sheet >= count || item.row < 0 || item.row >= 1048576 || item.col < 0 || item.col >= 16384) throw new Error('coordinate outside workbook');
      if (field === 'outputs') continue;
      const value = field === 'cells' ? item.input : item.value;
      if (!(suite.schema_version === 2 && value && typeof value === 'object' && Object.keys(value).length === 1 && typeof value.text === 'string') && !(typeof value === 'number' && Number.isFinite(value)) && !(typeof value === 'string' && value.startsWith('=') && value.length > 1)) throw new Error('invalid source input');
      if (suite.schema_version === 1 && field === 'edits' && typeof value !== 'number') throw new Error('formula edits require schema2');
    }
  }
}

function main() {
  const suite = JSON.parse(fs.readFileSync(0, 'utf8'));
  if (![1, 2].includes(suite.schema_version) || !Number.isInteger(suite.samples) || suite.samples < 1 ||
      !Number.isInteger(suite.warmup) || suite.warmup < 0 || !suite.cases.length) {
    throw new Error('unsupported or empty benchmark suite');
  }
  validateSuite(suite);
  const arrayArithmetic = process.argv[2] === 'array';
  const timings = [];
  for (const test of suite.cases) {
    for (let round = 0; round < suite.warmup + suite.samples; round++) {
      let start = process.hrtime.bigint();
      const model = build(test, arrayArithmetic);
      try {
        const actual = read(model, test.outputs);
        const elapsed = process.hrtime.bigint() - start;
        validate(actual, test.expected, test.name);
        if (round >= suite.warmup) timings.push({ case: test.name,
          phase: 'build_calculate_read', sample: round - suite.warmup,
          edit: null, elapsed_ns: Number(elapsed) });
        if (suite.unchanged_read) {
          start = process.hrtime.bigint();
          const unchanged = read(model, test.outputs);
          const elapsedRead = process.hrtime.bigint() - start;
          validate(unchanged, test.expected, test.name);
          if (round >= suite.warmup) timings.push({case: test.name,
            phase: 'unchanged_read', sample: round - suite.warmup,
            edit: null, elapsed_ns: Number(elapsedRead)});
        }
        for (const [index, change] of test.edits.entries()) {
          start = process.hrtime.bigint();
          // Native setter recalculates affected formulas before returning.
          model.setCellContents({ sheet: change.sheet || 0, row: change.row, col: change.col }, [[typeof change.value === 'object' ? "'" + change.value.text : change.value]]);
          const values = read(model, test.outputs);
          const elapsedEdit = process.hrtime.bigint() - start;
          validate(values, change.expected, test.name);
          if (round >= suite.warmup) timings.push({ case: test.name,
            phase: 'edit_calculate_read', sample: round - suite.warmup,
            edit: index, elapsed_ns: Number(elapsedEdit) });
        }
      } finally {
        model.destroy();
      }
    }
  }
  process.stdout.write(JSON.stringify({ schema_version: suite.schema_version, engine: 'hyperformula',
    version, mode: arrayArithmetic ? 'automatic-array-arithmetic' : 'automatic', timings }));
}

try { main(); } catch (error) {
  process.stderr.write(`${error.stack || error}\n`);
  process.exitCode = 1;
}
