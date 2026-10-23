// SPDX-License-Identifier: MIT
// Original protocol harness; upstream APIs and provenance are in ../LICENSES.md.
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Correctness-gated lifecycle shared by the isolated Java comparison adapters. */
public final class Bench {
    /** A native workbook whose disposal occurs outside the measurement. */
    public interface Model extends AutoCloseable {
        /** Calculate and collect the requested numeric outputs. */
        double[] read(JsonNode outputs) throws Exception;
        /** Collect retained results without explicitly requesting recalculation. */
        double[] readUnchanged(JsonNode outputs) throws Exception;
        /** Replace one literal or formula source and notify the native calculation engine. */
        void edit(JsonNode change) throws Exception;
        /** Release native model resources outside timing. */
        void close() throws Exception;
    }
    /** Native runtime initialized once, before any model timing. */
    public interface Engine extends AutoCloseable {
        /** Construct and populate a fresh in-memory workbook. */
        Model build(JsonNode test) throws Exception;
        /** Return the actual loaded engine release. */
        String version() throws Exception;
        /** Identify the engine in the common report. */
        String name();
        /** Identify the public calculation workflow. */
        String mode();
        /** Shut down runtime resources after the suite. */
        void close() throws Exception;
    }
    private static void validate(double[] actual, JsonNode expected, String name) {
        if (!expected.isArray() || actual.length == 0 || actual.length != expected.size())
            throw new IllegalArgumentException(name + ": output count mismatch");
        for (int i = 0; i < actual.length; i++) {
            double reference = expected.get(i).asDouble();
            if (!expected.get(i).isNumber() || !Double.isFinite(reference) || !Double.isFinite(actual[i]) ||
                Math.abs(actual[i] - reference) > 1e-9 + 1e-10 * Math.abs(reference))
                throw new IllegalArgumentException(name + ": output " + i + ": expected " + reference + ", got " + actual[i]);
        }
    }
    private static Map<String, Object> timing(String name, String phase, int sample, Integer edit, long elapsed) {
        Map<String, Object> item = new LinkedHashMap<>();
        item.put("case", name); item.put("phase", phase); item.put("sample", sample);
        item.put("edit", edit); item.put("elapsed_ns", elapsed);
        return item;
    }
    private static void validateSuite(JsonNode suite) {
        int schema = suite.path("schema_version").asInt();
        if (schema == 1 && suite.path("unchanged_read").asBoolean(false))
            throw new IllegalArgumentException("unchanged_read requires schema2");
        for (JsonNode test : suite.get("cases")) {
            if (test.has("sheet_count") && (!test.get("sheet_count").isIntegralNumber() || !test.get("sheet_count").canConvertToInt()))
                throw new IllegalArgumentException("invalid sheet count");
            int count = test.path("sheet_count").asInt(1);
            if (count < 1 || (schema == 1 && count != 1)) throw new IllegalArgumentException("multiple sheets require schema2");
            for (String field : new String[]{"cells", "outputs", "edits"}) {
                for (JsonNode item : test.get(field)) {
                    for (String coordinate : new String[]{"row", "col", "sheet"}) {
                        if (coordinate.equals("sheet") && !item.has("sheet")) continue;
                        if (!item.path(coordinate).isIntegralNumber() || !item.path(coordinate).canConvertToInt())
                            throw new IllegalArgumentException("invalid coordinate");
                    }
                    int sheet = item.path("sheet").asInt(0), row = item.path("row").asInt(-1), col = item.path("col").asInt(-1);
                    if (sheet < 0 || sheet >= count || row < 0 || row >= 1048576 || col < 0 || col >= 16384)
                        throw new IllegalArgumentException("coordinate outside workbook");
                    if (!field.equals("outputs")) {
                        JsonNode value = item.path(field.equals("cells") ? "input" : "value");
                        if (!(schema == 2 && value.isObject() && value.size() == 1 && value.path("text").isTextual()) && !(value.isNumber() && Double.isFinite(value.asDouble())) &&
                            !(value.isTextual() && value.asText().startsWith("=") && value.asText().length() > 1))
                            throw new IllegalArgumentException("invalid source input");
                        if (schema == 1 && field.equals("edits") && !value.isNumber())
                            throw new IllegalArgumentException("formula edits require schema2");
                    }
                }
            }
        }
    }
    /** Read one suite, validate every round, and publish only a complete report. */
    public static void main(String[] args) {
        try {
            if (args.length != 1) throw new IllegalArgumentException("expected engine adapter class");
            ObjectMapper json = new ObjectMapper();
            JsonNode suite = json.readTree(System.in);
            if ((suite.path("schema_version").asInt() != 1 && suite.path("schema_version").asInt() != 2) || !suite.path("samples").isIntegralNumber() ||
                suite.path("samples").asInt() < 1 || !suite.path("warmup").isIntegralNumber() ||
                suite.path("warmup").asInt(-1) < 0 || !suite.path("cases").isArray() || suite.path("cases").isEmpty())
                throw new IllegalArgumentException("unsupported or empty benchmark suite");
            validateSuite(suite);
            List<Map<String, Object>> timings = new ArrayList<>();
            Map<String, Object> report;
            try (Engine engine = (Engine) Class.forName(args[0]).getConstructor().newInstance()) {
                String version = engine.version();
                int warmup = suite.get("warmup").asInt(), samples = suite.get("samples").asInt();
                for (JsonNode test : suite.get("cases")) {
                    String name = test.path("name").asText();
                    for (int round = 0; round < warmup + samples; round++) {
                        long start = System.nanoTime();
                        try (Model model = engine.build(test)) {
                            double[] values = model.read(test.get("outputs"));
                            long elapsed = System.nanoTime() - start;
                            validate(values, test.get("expected"), name);
                            if (round >= warmup) timings.add(timing(name, "build_calculate_read", round - warmup, null, elapsed));
                            if (suite.path("unchanged_read").asBoolean(false)) {
                                start = System.nanoTime();
                                values = model.readUnchanged(test.get("outputs"));
                                elapsed = System.nanoTime() - start;
                                validate(values, test.get("expected"), name);
                                if (round >= warmup) timings.add(timing(name, "unchanged_read", round - warmup, null, elapsed));
                            }
                            int step = 0;
                            for (JsonNode change : test.get("edits")) {
                                start = System.nanoTime();
                                model.edit(change);
                                values = model.read(test.get("outputs"));
                                elapsed = System.nanoTime() - start;
                                validate(values, change.get("expected"), name);
                                if (round >= warmup) timings.add(timing(name, "edit_calculate_read", round - warmup, step, elapsed));
                                step++;
                            }
                        }
                    }
                }
                report = Map.of("schema_version", suite.get("schema_version").asInt(), "engine", engine.name(),
                    "version", version, "mode", engine.mode(), "timings", timings);
            }
            // Serialize after all correctness checks and runtime cleanup succeed.
            System.out.println(json.writeValueAsString(report));
        } catch (Exception error) {
            error.printStackTrace(System.err);
            System.exit(1);
        }
    }
}
