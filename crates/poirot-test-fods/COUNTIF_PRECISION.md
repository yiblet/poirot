# COUNTIF text-criterion precision difference

`statistical__fods__countif2__00008` retains LibreOffice's expected **1** for
`COUNTIF(J10:J10,"="&J10)`, where J10 represents 86.47 seconds as a day fraction.
Poirot returns **0**, matching a fresh Apache POI 5.4.1 evaluation. Microsoft
Excel has not been executed for this case.

The raw number is `0.001000810185185185`. Spreadsheet number-to-text conversion
produces `0.00100081018518519`. POI's `Countif.NumberMatcher` compares the parsed
criterion and the original number using `Double.compare`, so they differ.
`COUNTIF(J10:J10,J10)` passes a numeric criterion directly and returns **1** in
POI and Poirot. This difference is independent of the cell's display format.

Correcting numeric concatenation exposed this engine difference; the previous
long binary-float text happened to satisfy LibreOffice's fixture. We retain
POI-compatible exact criteria comparison rather than add a tolerance solely to
make this fixture pass. This is a documented POI quirk, not evidence that the
LibreOffice expected value is wrong or that Excel behaves like POI.

Only this case's baseline classification changes to a known numeric gap. Its
native fixture bytes, source expectation, and integrity digest are unchanged.
Default runs still attempt and report it; `--strict` still fails for it. The
fourteen improvements from the rounding and text changes remain improvements;
the three pre-existing regressions are not reclassified.

The evaluator's `countif_text_precision` regression reproduces the raw cell,
concatenated criterion, and both COUNTIF forms under default and pinned POI
policies.

Reference implementation: Apache POI 5.4.1
[`Countif.NumberMatcher`](https://github.com/apache/poi/blob/REL_5_4_1/poi/src/main/java/org/apache/poi/ss/formula/functions/Countif.java)
and
[`NumberToTextConverter`](https://github.com/apache/poi/blob/REL_5_4_1/poi/src/main/java/org/apache/poi/ss/util/NumberToTextConverter.java).
