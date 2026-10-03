"""Link independently generated fixtures to the coverage inventory and evidence.

Function mention is never treated as full contract coverage. Only the primary
behavior exercised by a fixture is linked; uncovered obligations remain visible.
"""

import copy
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SPILL = "https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior"


def legacy_obligations(case, known):
    formula = next(cell["formula"] for cell in case["cells"]
                   if (cell["sheet"], cell["address"]) ==
                   (case["target"]["sheet"], case["target"]["address"]))
    calls = re.findall(r"([A-Za-z][A-Za-z0-9_.]*)\s*\(", formula)
    links = []
    if case["name"].startswith("explicit_intersect"):
        links.append("language.explicit_intersection.disjoint" if case["expected"]["type"] == "error"
                     else "language.explicit_intersection.overlap")
    elif case["name"].startswith("implicit_"):
        links.append("language.implicit_range.horizontal" if "horizontal" in case["name"]
                     else "language.implicit_range.vertical")
        if "@" in formula:
            links.append("language.implicit_range.explicit_at")
    elif case["name"].startswith("index_bounds"):
        links.append("function.INDEX.reference")
    elif case["category"] == "dynamic_references" and len(calls) > 1 and calls[0].upper() == "SUM":
        links.append(f"function.{calls[1].upper()}.reference")
    elif calls:
        facet = "errors" if case["expected"]["type"] == "error" else "valid"
        links.append(f"function.{calls[0].upper()}.{facet}")
    if case["name"].startswith("if_lazy"):
        links.append("workbook.branch_switches.lazy")
    if case["name"].startswith("chain_"):
        links.append("workbook.dependencies.chain")
    if case["category"] == "unicode":
        texts = [cell.get("value", {}).get("value", "") for cell in case["cells"]
                 if cell.get("value", {}).get("type") == "text"]
        if any(ord(char) > 65535 for text in texts for char in text):
            links.append("workbook.utf16_v2.pairs" if case["text_policy"] == "excel_v2"
                         else "workbook.utf16_v1.pairs")
        if case["expected"]["type"] == "utf16":
            links.append("workbook.utf16_v1.isolated")
    # A source-backed legacy fixture may not establish all possible facets.
    return sorted(set(link for link in links if link in known))


def normalize_legacy(case, known):
    case = copy.deepcopy(case)
    evidence = {"kind": "documented_derivation", "source_urls": case["source_urls"],
                "explanation": case["rationale"],
                "review": "Legacy fixture independently reviewed before ROI execution; no Excel observation claimed."}
    assertion = {"target": case.pop("target"), "expected": case.pop("expected"),
                 "evidence": evidence, "obligations": []}
    # Mapping needs the original target and expected fields.
    original = case | {"target": assertion["target"], "expected": assertion["expected"]}
    assertion["obligations"] = legacy_obligations(original, known)
    if "tolerance" in case:
        assertion["tolerance"] = case.pop("tolerance")
    case["schema_version"] = 2
    case["settings"] = {"date_system": "1904" if case.pop("date1904", False) else "1900"}
    case["names"] = []
    case["steps"] = [{"edits": [], "assertions": [assertion]}]
    return case


def assemble(cases, baseline_sources):
    # The one committed JSON table is the source of authored scenarios. The
    # reviewed matrix prefix is regenerated independently; authored cases are
    # preserved rather than duplicated in a second Python data dump.
    assert len(cases) == 1500, "Keep baseline IDs stable; append authored cases."
    existing = json.loads((ROOT / "cases.json").read_text())
    authored = [case for case in existing if case["id"] >= len(cases)]

    inventory = json.loads((ROOT / "inventory.json").read_text())
    known = {obligation["id"] for feature in inventory["features"] for obligation in feature["obligations"]}
    combined = [normalize_legacy(case, known) for case in cases] + copy.deepcopy(authored)
    combined.sort(key=lambda case: case["id"])
    assert [case["id"] for case in combined] == list(range(len(combined)))
    for case in combined:
        case["schema_version"] = 2
        if not case["name"].endswith(f"_{case['id']:04d}"):
            case["name"] += f"_{case['id']:04d}"
        case.setdefault("settings", {"date_system": "1900"})
        case.setdefault("names", [])
        for step in case["steps"]:
            for assertion in step["assertions"]:
                assertion["evidence"]["source_urls"] = sorted(set(assertion["evidence"]["source_urls"]))
                if assertion.get("expected", {}).get("type") == "array":
                    evidence = assertion["evidence"]
                    if SPILL not in evidence["source_urls"]:
                        evidence["source_urls"].append(SPILL)
                    assertion["obligations"] = sorted(set(assertion["obligations"] +
                        ["workbook.array_results.shape", "language.spill.geometry"]))
                assert assertion["obligations"] and set(assertion["obligations"]) <= known, case["id"]
        case["source_urls"] = sorted(set(case["source_urls"]) |
            {url for step in case["steps"] for assertion in step["assertions"]
             for url in assertion["evidence"]["source_urls"]})
    baseline = {source["url"]: source for source in baseline_sources}
    sources = []
    for url in sorted({url for case in combined for url in case["source_urls"]}):
        source = copy.deepcopy(baseline.get(url, {
            "url": url, "publisher": "Microsoft", "verified_on": "2026-10-13",
            "applicability": "Primary documentation opened by the author/reviewer; assertion evidence identifies the specific rule and derivation.",
        }))
        source["spec_ids"] = [case["id"] for case in combined if url in case["source_urls"]]
        sources.append(source)
    def normalize_blanks(node):
        if isinstance(node, dict):
            if node.get("type") == "blank":
                node.pop("value", None)
            for child in node.values():
                normalize_blanks(child)
        elif isinstance(node, list):
            for child in node:
                normalize_blanks(child)
    normalize_blanks(combined)
    (ROOT / "cases.json").write_text(json.dumps(combined, ensure_ascii=True, indent=2) + "\n")
    (ROOT / "sources.json").write_text(json.dumps(sources, indent=2) + "\n")
    return combined
