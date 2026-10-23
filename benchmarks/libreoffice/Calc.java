// SPDX-License-Identifier: MIT
import com.fasterxml.jackson.databind.JsonNode;
import com.sun.star.beans.PropertyValue;
import com.sun.star.beans.XPropertySet;
import com.sun.star.bridge.XUnoUrlResolver;
import com.sun.star.comp.helper.Bootstrap;
import com.sun.star.container.XIndexAccess;
import com.sun.star.frame.XComponentLoader;
import com.sun.star.frame.XDesktop;
import com.sun.star.lang.XComponent;
import com.sun.star.lang.XMultiServiceFactory;
import com.sun.star.sheet.AddressConvention;
import com.sun.star.sheet.FormulaLanguage;
import com.sun.star.sheet.FormulaMapGroup;
import com.sun.star.sheet.XFormulaOpCodeMapper;
import com.sun.star.sheet.XCalculatable;
import com.sun.star.sheet.XCellRangeData;
import com.sun.star.sheet.XFormulaParser;
import com.sun.star.sheet.XFormulaTokens;
import com.sun.star.sheet.XSpreadsheet;
import com.sun.star.sheet.XSpreadsheetDocument;
import com.sun.star.sheet.XSpreadsheets;
import com.sun.star.table.CellAddress;
import com.sun.star.table.XCell;
import com.sun.star.text.XText;
import com.sun.star.uno.UnoRuntime;
import com.sun.star.uno.XComponentContext;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.concurrent.TimeUnit;

/** An isolated headless Calc process, connected before any measured operation. */
public final class Calc implements Bench.Engine {
    private final Path profile;
    private final Process office;
    private final XComponentContext context;
    private final XDesktop desktop;
    private final XComponentLoader loader;
    /** Start a private office process and establish its local UNO pipe. */
    public Calc() throws Exception {
        profile = Files.createTempDirectory("poirot-calc-");
        String pipe = "poirot_" + ProcessHandle.current().pid();
        office = new ProcessBuilder("/opt/libreoffice26.8/program/soffice", "--headless", "--nologo", "--nodefault", "--norestore", "--nofirststartwizard",
            "-env:UserInstallation=" + profile.toUri(), "--accept=pipe,name=" + pipe + ";urp;StarOffice.ServiceManager")
            .redirectOutput(ProcessBuilder.Redirect.DISCARD).redirectError(ProcessBuilder.Redirect.INHERIT).start();
        XComponentContext local = Bootstrap.createInitialComponentContext(null);
        XUnoUrlResolver resolver = query(XUnoUrlResolver.class,
            local.getServiceManager().createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", local));
        XComponentContext connected = null;
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(30);
        while (connected == null && System.nanoTime() < deadline) {
            try { connected = query(XComponentContext.class, resolver.resolve("uno:pipe,name=" + pipe + ";urp;StarOffice.ComponentContext")); }
            catch (com.sun.star.connection.NoConnectException error) {
                if (!office.isAlive()) throw new IllegalStateException("LibreOffice exited before connection", error);
                Thread.sleep(50);
            }
        }
        if (connected == null) { office.destroyForcibly(); throw new IllegalStateException("UNO connection timeout"); }
        context = connected;
        Object nativeDesktop = context.getServiceManager().createInstanceWithContext("com.sun.star.frame.Desktop", context);
        desktop = query(XDesktop.class, nativeDesktop);
        loader = query(XComponentLoader.class, nativeDesktop);
    }
    private static <T> T query(Class<T> type, Object value) {
        T result = UnoRuntime.queryInterface(type, value);
        if (result == null) throw new IllegalArgumentException("UNO object lacks " + type.getName());
        return result;
    }
    private static PropertyValue property(String name, Object value) {
        PropertyValue property = new PropertyValue(); property.Name = name; property.Value = value; return property;
    }
    /** Build an in-memory document; no spreadsheet codec or file path is used. */
    public Bench.Model build(JsonNode test) throws Exception { return new Workbook(test); }
    /** Read the running office's configured product version. */
    public String version() throws Exception {
        Object provider = context.getServiceManager().createInstanceWithContext("com.sun.star.configuration.ConfigurationProvider", context);
        Object access = query(XMultiServiceFactory.class, provider).createInstanceWithArguments(
            "com.sun.star.configuration.ConfigurationAccess", new Object[] { property("nodepath", "/org.openoffice.Setup/Product") });
        return query(XPropertySet.class, access).getPropertyValue("ooSetupVersionAboutBox").toString();
    }
    /** Identify LibreOffice Calc. */
    public String name() { return "libreoffice"; }
    /** Identify explicit dirty recalculation over the Java UNO bridge. */
    public String mode() { return "uno-dirty"; }
    /** Stop the private process and delete its temporary profile. */
    public void close() throws Exception {
        try { desktop.terminate(); } finally {
            if (!office.waitFor(5, TimeUnit.SECONDS)) office.destroyForcibly().waitFor();
            try (var paths = Files.walk(profile)) {
                for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path);
            }
        }
    }
    /** A fresh Calc document, with automatic calculation disabled for both phases. */
    private final class Workbook implements Bench.Model {
        private final XComponent document;
        private final XCalculatable calculation;
        private final XSpreadsheet[] sheetCache;
        private final XFormulaParser parser;
        private Workbook(JsonNode test) throws Exception {
            document = loader.loadComponentFromURL("private:factory/scalc", "_blank", 0,
                new PropertyValue[] { property("Hidden", true) });
            try {
                calculation = query(XCalculatable.class, document);
                calculation.enableAutomaticCalculation(false);
                XSpreadsheets sheets = query(XSpreadsheetDocument.class, document).getSheets();
                int sheetCount = test.path("sheet_count").asInt(1);
                for (int i = 1; i < sheetCount; i++) sheets.insertNewByName("Sheet" + (i + 1), (short) i);
                sheetCache = new XSpreadsheet[sheetCount];
                XIndexAccess indexedSheets = query(XIndexAccess.class, sheets);
                for (int i = 0; i < sheetCount; i++) sheetCache[i] = query(XSpreadsheet.class, indexedSheets.getByIndex(i));
                Object parserObject = query(XMultiServiceFactory.class, document).createInstance("com.sun.star.sheet.FormulaParser");
                XPropertySet settings = query(XPropertySet.class, parserObject);
                settings.setPropertyValue("CompileEnglish", true);
                settings.setPropertyValue("FormulaConvention", AddressConvention.XL_A1);
                XFormulaOpCodeMapper mapper = query(XFormulaOpCodeMapper.class,
                    query(XMultiServiceFactory.class, document).createInstance("com.sun.star.sheet.FormulaOpCodeMapper"));
                settings.setPropertyValue("OpCodeMap", mapper.getAvailableMappings(FormulaLanguage.XL_ENGLISH, FormulaMapGroup.ALL_EXCEPT_SPECIAL));
                parser = query(XFormulaParser.class, parserObject);
                for (JsonNode item : test.get("cells")) {
                    int row = item.get("row").asInt(), col = item.get("col").asInt();
                    int sheetIndex = item.path("sheet").asInt(0);
                    XCell cell = sheet(sheetIndex).getCellByPosition(col, row);
                    JsonNode input = item.get("input");
                    if (input.isObject() && input.path("text").isTextual()) query(XText.class, cell).setString(input.get("text").asText());
                    else if (input.isNumber()) cell.setValue(input.asDouble());
                    else if (input.isTextual() && input.asText().startsWith("=")) {
                        CellAddress origin = new CellAddress((short) sheetIndex, col, row);
                        query(XFormulaTokens.class, cell).setTokens(parser.parseFormula(input.asText(), origin));
                    } else throw new IllegalArgumentException("unsupported shared input");
                }
            } catch (Exception error) { document.dispose(); throw error; }
        }
        /** Calculate dirty formulas, then collect values through the UNO bridge. */
        public double[] read(JsonNode outputs) throws Exception {
            calculation.calculate();
            return readUnchanged(outputs);
        }
        private XSpreadsheet sheet(int index) {
            if (index < 0 || index >= sheetCache.length) throw new IllegalArgumentException("sheet outside workbook");
            return sheetCache[index];
        }
        /** Collect current native cell values without calculate(). */
        public double[] readUnchanged(JsonNode outputs) throws Exception {
            double[] values = new double[outputs.size()];
            int index = 0;
            for (JsonNode at : outputs) {
                XCell cell = sheet(at.path("sheet").asInt(0)).getCellByPosition(at.get("col").asInt(), at.get("row").asInt());
                int error = cell.getError();
                if (error != 0) throw new IllegalArgumentException("Calc output error=" + error);
                Object value = query(XCellRangeData.class, cell).getDataArray()[0][0];
                if (!(value instanceof Double)) throw new IllegalArgumentException("nonnumeric Calc formula result at row=" + at.get("row").asInt() + ",col=" + at.get("col").asInt() + ": " + value);
                values[index++] = (Double) value;
            }
            return values;
        }
        /** Replace the authored source; the next read triggers native dirty recalculation. */
        public void edit(JsonNode change) throws Exception {
            int sheetIndex = change.path("sheet").asInt(0), col = change.get("col").asInt(), row = change.get("row").asInt();
            XCell cell = sheet(sheetIndex).getCellByPosition(col, row);
            JsonNode value = change.get("value");
            if (value.isObject() && value.path("text").isTextual()) query(XText.class, cell).setString(value.get("text").asText());
            else if (value.isNumber()) cell.setValue(value.asDouble());
            else if (value.isTextual() && value.asText().startsWith("="))
                query(XFormulaTokens.class, cell).setTokens(parser.parseFormula(value.asText(), new CellAddress((short) sheetIndex, col, row)));
            else throw new IllegalArgumentException("invalid source edit");
        }
        /** Dispose outside the measurement. */
        public void close() { document.dispose(); }
    }
}
