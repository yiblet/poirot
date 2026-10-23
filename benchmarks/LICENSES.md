# Benchmark engine licenses and provenance

These comparison tools are separate from Poirot's library dependency graph.
The user explicitly authorized copyleft engines for this benchmark suite.
Workloads and adapters were written for Poirot; no upstream engine code,
benchmark algorithms, or fixtures were copied or translated.

| Component | Pinned source | License selected | Use |
| --- | --- | --- | --- |
| Poirot and original Rust/Python benchmark tools | Local checkout; revision and worktree fingerprint recorded in results | MIT, [license text](../LICENSES/MIT.txt) | Model generation, orchestration, Rust harness, and Poirot adapter. |
| IronCalc | [`ironcalc_base` 0.9.0](https://crates.io/crates/ironcalc_base/0.9.0), revision [`297553dc3a37ec6b0f4821055220e3d26c4ac6a1`](https://github.com/ironcalc/IronCalc/tree/297553dc3a37ec6b0f4821055220e3d26c4ac6a1/base) | MIT option of MIT OR Apache-2.0 | Calls to the public raw `Model` API in a separate executable/image. |
| HyperFormula | [3.5.0](https://www.npmjs.com/package/hyperformula/v/3.5.0), revision [`99a45ea1d7ee88755607d8ae0153fb6d13efc36c`](https://github.com/handsontable/hyperformula/tree/99a45ea1d7ee88755607d8ae0153fb6d13efc36c) | GPL-3.0-only, configured with `licenseKey: 'gpl-v3'` | Separate Node adapter/image; the original `hyperformula/adapter.cjs` is GPL-3.0-only too. |

Cargo and npm lockfiles record exact transitive versions and integrity hashes.
Rust images retain fetched dependency license/notice files and Poirot's existing
attributions under `/licenses`. Node images retain upstream license files in
`/benchmark/node_modules` and the adapter's GPLv3 text in `/benchmark/COPYING`.
No upstream copyright or license notices are removed.

The GPLv3 text in [hyperformula/COPYING](hyperformula/COPYING) is reproduced from
the [Free Software Foundation's GPLv3 text](https://www.gnu.org/licenses/gpl-3.0.txt),
version 3, 29 June 2007. HyperFormula's licensing choice is documented in its
[license-key guide](https://handsontable.com/docs/hyperformula/guide/license-key).
Its original package license notice remains in the installed dependency.

The Dockerfiles use official Rust, Debian, and Node images. Results record the
built image IDs and Dockerfile hashes; base tags may receive upstream updates.

Apache POI 5.5.1 is fetched from [Maven Central](https://repo.maven.apache.org/maven2/org/apache/poi/poi-ooxml/5.5.1/)
under Apache-2.0. Its original `LICENSE`/`NOTICE` and those of its dependencies are
extracted into `/licenses/<jar-name>/` and remain inside the unmodified jars.
The [official download page](https://poi.apache.org/download.html) identifies this
release. `apache-poi/Poi.java` and `java/Bench.java` are original MIT adapters;
no upstream implementation, examples, tests, or fixture material was copied.
Jackson 2.20.1 (annotations 2.20), used only for JSON, selects Apache-2.0.
The pinned Maven dependency plugin and POM resolve Java dependencies, then
[java/artifacts.sha256](java/artifacts.sha256) verifies every runtime jar against
the recorded SHA-256. Neither adapter adds Java dependencies to Poirot.

LibreOffice 26.8.1.1 is installed from The Document Foundation's official
[26.8.1 x86-64 Debian archive](https://download.documentfoundation.org/libreoffice/stable/26.8.1/deb/x86_64/LibreOffice_26.8.1_Linux_x86-64_deb.tar.gz).
Its official mirror-service SHA-256 is
`30903df3b9f61360d9660cd707de48cd2831469114492a5008ed58a0ac77d044`, checked
before extraction. Corresponding source is available in the official
[26.8.1.1 source archive](https://download.documentfoundation.org/libreoffice/src/26.8.1/libreoffice-26.8.1.1.tar.xz).
The image retains the complete installed `/opt/libreoffice26.8`
distribution, including licenses/notices and matching Java UNO jars; package
copyrights are also retained under `/licenses/distribution-docs/`. Installed
package versions, artifact hashes and native CLI/JVM versions are in `/licenses`.
LibreOffice uses [MPL-2.0 and bundled component licenses](https://www.libreoffice.org/licenses/);
the explicitly permitted copyleft use is confined to this comparison image.
`libreoffice/Calc.java` is an original MIT adapter calling public UNO APIs; no
upstream algorithms, examples, or tests were copied. Its grammar configuration
follows the documented [FormulaParser properties](https://api.libreoffice.org/docs/idl/ref/servicecom_1_1sun_1_1star_1_1sheet_1_1FormulaParser.html),
and dirty calculation follows [XCalculatable](https://api.libreoffice.org/docs/idl/ref/interfacecom_1_1sun_1_1star_1_1sheet_1_1XCalculatable.html).
Java adapters use Eclipse Temurin 21 and Maven official images. Their base tags
and Ubuntu support packages can receive updates; recorded image IDs, runtime
versions and package manifests identify the actual build.
