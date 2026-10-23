// SPDX-License-Identifier: MIT
import com.fasterxml.jackson.databind.JsonNode;
import org.apache.poi.Version;
import org.apache.poi.ss.usermodel.Cell;
import org.apache.poi.ss.usermodel.CellType;
import org.apache.poi.ss.usermodel.CellValue;
import org.apache.poi.ss.usermodel.FormulaEvaluator;
import org.apache.poi.ss.usermodel.Row;
import org.apache.poi.ss.usermodel.Sheet;
import org.apache.poi.xssf.usermodel.XSSFWorkbook;

/** Apache POI's public in-memory OOXML workbook and cached formula evaluator. */
public final class Poi implements Bench.Engine {
    /** Initialize the adapter; JVM startup is outside measurement. */
    public Poi() {}
    /** Construct the workbook and parse its original shared formulas. */
    public Bench.Model build(JsonNode test) throws Exception { return new Workbook(test); }
    /** Return the release reported by the loaded POI classes. */
    public String version() { return Version.getVersion(); }
    /** Identify Apache POI. */
    public String name() { return "apache-poi"; }
    /** Identify its demand-driven cached evaluator. */
    public String mode() { return "formula-evaluator"; }
    /** No engine process exists beyond the JVM. */
    public void close() {}

    /** One fresh native workbook retained across both edits. */
    private static final class Workbook implements Bench.Model {
        private final XSSFWorkbook book = new XSSFWorkbook();

        private final FormulaEvaluator evaluator;
        private Workbook(JsonNode test) throws Exception {
            try {
                for (int i = 0; i < test.path("sheet_count").asInt(1); i++) book.createSheet("Sheet" + (i + 1));
                for (JsonNode item : test.get("cells")) {
                    Sheet sheet = book.getSheetAt(item.path("sheet").asInt(0));
                    int r = item.get("row").asInt(), c = item.get("col").asInt();
                    Row row = sheet.getRow(r);
                    if (row == null) row = sheet.createRow(r);
                    Cell cell = row.createCell(c);
                    JsonNode input = item.get("input");
                    if (input.isObject() && input.path("text").isTextual()) cell.setCellValue(input.get("text").asText());
                    else if (input.isNumber()) cell.setCellValue(input.asDouble());
                    else if (input.isTextual() && input.asText().startsWith("=")) cell.setCellFormula(input.asText().substring(1));
                    else throw new IllegalArgumentException("unsupported shared input");
                }
                evaluator = book.getCreationHelper().createFormulaEvaluator();
            } catch (Exception error) { book.close(); throw error; }
        }
        /** Evaluate requested outputs with native caching and preserve their order. */
        public double[] read(JsonNode outputs) {
            double[] result = new double[outputs.size()];
            int index = 0;
            for (JsonNode at : outputs) {
                Sheet sheet = book.getSheetAt(at.path("sheet").asInt(0));
                Row row = sheet.getRow(at.get("row").asInt());
                Cell cell = row == null ? null : row.getCell(at.get("col").asInt());
                if (cell == null) throw new IllegalArgumentException("blank POI output");
                CellValue value = evaluator.evaluate(cell);
                if (value == null || value.getCellType() != CellType.NUMERIC)
                    throw new IllegalArgumentException("nonnumeric POI output: " + value);
                result[index++] = value.getNumberValue();
            }
            return result;
        }
        /** Reuse the evaluator cache without an explicit invalidation or recalculation. */
        public double[] readUnchanged(JsonNode outputs) { return read(outputs); }
        /** Notify exactly the changed source, retaining unrelated evaluator caches. */
        public void edit(JsonNode change) {
            Sheet sheet = book.getSheetAt(change.path("sheet").asInt(0));
            int r = change.get("row").asInt(), c = change.get("col").asInt();
            Row row = sheet.getRow(r);
            if (row == null) row = sheet.createRow(r);
            Cell cell = row.getCell(c);
            if (cell == null) cell = row.createCell(c);
            JsonNode value = change.get("value");
            if (!value.isTextual() && cell.getCellType() == CellType.FORMULA) cell.setBlank();
            if (value.isObject() && value.path("text").isTextual()) cell.setCellValue(value.get("text").asText());
            else if (value.isNumber()) cell.setCellValue(value.asDouble());
            else if (value.isTextual() && value.asText().startsWith("=")) cell.setCellFormula(value.asText().substring(1));
            else throw new IllegalArgumentException("invalid source edit");
            evaluator.notifyUpdateCell(cell);
        }
        /** Dispose outside the measured phase. */
        public void close() throws Exception { book.close(); }
    }
}
