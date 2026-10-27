"""Check evaluator folder dependencies against their `Depends on:` docs (Python 3.11+).

Run `python3 tools/test_eval_dependencies.py`. Every folder under `src/eval/` states
in its `mod.rs` `//!` doc a `Depends on:` paragraph naming the top-level evaluator
folders its non-test code uses (`no other eval folder` when there are none). The
check fails when:

- `undocumented-edge`: non-test code in a folder uses a top-level folder that its
  `Depends on:` paragraph does not name;
- `unused-edge`: the paragraph names a folder that its non-test code does not use;
- `missing-depends-on`: a folder `mod.rs` has no `Depends on:` paragraph;
- `cycle`: top-level folders depend on each other in a cycle that is not contained in
  `RECURSION_GROUP`, the one documented recursion group (cells run formulas, formulas
  call adapters, adapters read cells and spills).

Folder names inside parentheses and after `sibling`/`siblings` are descriptions or
sibling modules, not top-level edges. Uses are found syntactically: `use` trees and
`crate::`/`super::`/`self::` paths, resolved through the eval root's re-exports.
Test code is excluded: `tests.rs` files, `tests/` folders, `#[cfg(test)]` items and
modules declared under `#[cfg(test)]`.
"""
from pathlib import Path
import re
import sys
import unittest

from test_coercion_architecture import mask_literals_and_comments
from test_module_layout import PATH, USE, matching, resolve_path, use_leaves

ROOT = Path(__file__).resolve().parents[1]
SCAN_ROOT = Path("src/eval")
CRATE = ("eval",)
RECURSION_GROUP = frozenset({"adapters", "host", "session", "spill", "vm"})
TEST_ATTRIBUTE = re.compile(r"#\[cfg\(test\)\]")


def module_path(path):
    """Return the module path below the eval root for a relative `.rs` path."""
    parts = Path(path).with_suffix("").parts
    return parts[:-1] if parts[-1] == "mod" else parts


def test_spans(code):
    """Return `(start, end)` spans of `#[cfg(test)]` items in masked code."""
    spans = []
    for attribute in TEST_ATTRIBUTE.finditer(code):
        index = attribute.end()
        while index < len(code) and code[index] not in ";{":
            index += 1
        if index < len(code) and code[index] == "{":
            index = matching(code, index, len(code))
        spans.append((attribute.start(), index + 1))
    return spans


def test_modules(path, code):
    """Return module paths of child files declared with `#[cfg(test)] mod name;`."""
    module = module_path(path)
    return {module + (match[1],) for match in re.finditer(
        r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;", code)}


def is_test_file(path, excluded):
    """Report whether a file is test code by name or by a test-only enclosing module."""
    parts = Path(path).parts
    if Path(path).name == "tests.rs" or "tests" in parts[:-1]:
        return True
    module = module_path(path)
    return any(module[:len(test)] == test for test in excluded)


def root_exports(code):
    """Map items re-exported by the eval root to the top-level folder defining them."""
    exported = {}
    for match in USE.finditer(code):
        for segments, glob in use_leaves(match[2]):
            if not glob and len(segments) > 1:
                exported[segments[-1]] = segments[0]
    return exported


def used_folders(path, code, folders, exported):
    """Return `{folder}` reached by non-test `use` trees and paths in one masked file."""
    spans = test_spans(code)
    live = "".join(" " if any(start <= index < end for start, end in spans) else char
                   for index, char in enumerate(code)) if spans else code
    scope = module_path(path)
    leaves = [segments for match in USE.finditer(live) for segments, _ in use_leaves(match[2])]
    leaves += [tuple(re.findall(r"\w+", match[0])) for match in PATH.finditer(live)]
    found = set()
    for segments in leaves:
        absolute = resolve_path(segments, scope, CRATE)
        if not absolute:
            continue
        folder = absolute[0] if absolute[0] in folders else exported.get(absolute[0])
        if folder and folder != scope[0]:
            found.add(folder)
    return found


def documented(source, folders):
    """Return the folders named by a `mod.rs` `Depends on:` paragraph, or `None`."""
    paragraphs, current = [], []
    for line in source.split("\n"):
        if not line.startswith("//!"):
            break
        text = line[3:].strip()
        if text:
            current.append(text)
        elif current:
            paragraphs.append(" ".join(current))
            current = []
    paragraphs.append(" ".join(current))
    paragraph = next((text.split("Depends on:", 1)[1] for text in paragraphs
                      if "Depends on:" in text), None)
    if paragraph is None:
        return None
    depth, outside = 0, []
    for char in paragraph:
        depth += char == "("
        outside.append(char if depth == 0 else " ")
        depth -= char == ")" and depth > 0
    paragraph = re.split(r"\bsiblings?\b", "".join(outside))[0]
    return {word for word in re.findall(r"[\w-]+", paragraph) if word in folders}


def strongly_connected(edges):
    """Return the strongly connected components of `{node: {target}}` (Tarjan)."""
    index, low, stack, on_stack, components = {}, {}, [], set(), []

    def visit(node):
        index[node] = low[node] = len(index)
        stack.append(node)
        on_stack.add(node)
        for target in sorted(edges.get(node, ())):
            if target not in index:
                visit(target)
                low[node] = min(low[node], low[target])
            elif target in on_stack:
                low[node] = min(low[node], index[target])
        if low[node] == index[node]:
            component = set()
            while True:
                member = stack.pop()
                on_stack.discard(member)
                component.add(member)
                if member == node:
                    break
            components.append(frozenset(component))

    for node in sorted(edges):
        if node not in index:
            visit(node)
    return components


def collect(sources):
    """Return violations and top-level edges for `{relative_path: source}` under the eval root."""
    folders = {Path(path).parts[0] for path in sources if len(Path(path).parts) > 1}
    masked = {path: mask_literals_and_comments(source) for path, source in sources.items()}
    excluded = set()
    for path, code in masked.items():
        excluded |= test_modules(path, code)
    exported = root_exports(masked.get("mod.rs", ""))
    uses = {}
    for path, code in sorted(masked.items()):
        if len(Path(path).parts) > 1 and not is_test_file(path, excluded):
            uses[path] = used_folders(path, code, folders, exported)
    found = []
    for path in sorted(sources):
        parts = Path(path).parts
        if parts[-1] != "mod.rs" or len(parts) < 2 or is_test_file(path, excluded):
            continue
        folder = parts[:-1]
        names = documented(sources[path], folders)
        if names is None:
            found.append(f"missing-depends-on {'/'.join(folder)}")
            continue
        names.discard(folder[0])
        actual = {}
        for used_path, targets in uses.items():
            if Path(used_path).parts[:len(folder)] == folder:
                for target in targets:
                    actual.setdefault(target, used_path)
        for target in sorted(set(actual) - names):
            found.append(f"undocumented-edge {'/'.join(folder)} -> {target} ({actual[target]})")
        for target in sorted(names - set(actual)):
            found.append(f"unused-edge {'/'.join(folder)} -> {target}")
    edges = {folder: set() for folder in folders}
    for path, targets in uses.items():
        edges[Path(path).parts[0]] |= targets
    for component in strongly_connected(edges):
        if len(component) > 1 and not component <= RECURSION_GROUP:
            found.append(f"cycle {' '.join(sorted(component))}")
    return found, edges


def scan(repository=ROOT, root=SCAN_ROOT):
    """Return violations and edges for the Rust files under `repository/root`."""
    base = repository / root
    return collect({str(path.relative_to(base)): path.read_text() for path in base.rglob("*.rs")})


class EvalDependencyTests(unittest.TestCase):
    """Rules on small synthetic trees, then the real evaluator tree."""

    ROOT_MOD = "mod low;\nmod mid;\nmod top;\npub use mid::Thing;\n"

    def tree(self, low="//! Depends on: no other eval folder.\n", mid="//! Depends on: low.\n",
             top="//! Depends on: mid (things) and low.\n", **files):
        sources = {"mod.rs": self.ROOT_MOD, "low/mod.rs": low, "mid/mod.rs": mid,
                   "top/mod.rs": top, "low/a.rs": "pub struct A;\n",
                   "mid/b.rs": "use crate::eval::low::A;\n",
                   "top/c.rs": "use crate::eval::{low, mid::B};\n"}
        sources.update(files)
        return collect(sources)

    def test_documented_layers_pass(self):
        found, edges = self.tree()
        self.assertEqual(found, [])
        self.assertEqual(edges, {"low": set(), "mid": {"low"}, "top": {"low", "mid"}})

    def test_undocumented_and_unused_edges(self):
        found, _ = self.tree(mid="//! Depends on: top (not really).\n")
        self.assertEqual(found, ["undocumented-edge mid -> low (mid/b.rs)",
                                 "unused-edge mid -> top"])

    def test_parenthesized_and_sibling_names_are_not_edges(self):
        found, _ = self.tree(
            top="//! Depends on: mid (unlike low here) and low; sibling top_inner.\n",
            **{"top/inner/mod.rs": "//! Depends on: no other eval folder; sibling low.\n"})
        self.assertEqual(found, [])

    def test_root_reexports_super_paths_and_inline_paths_resolve(self):
        found, edges = self.tree(**{
            "low/d.rs": "fn f() { let _ = crate::eval::Thing; }\n",
            "top/e.rs": "use super::super::mid;\nfn g() -> super::super::low::A { todo!() }\n"})
        self.assertEqual(edges["low"], {"mid"})
        self.assertIn("undocumented-edge low -> mid (low/d.rs)", found)
        self.assertIn("cycle low mid", found)

    def test_test_code_is_ignored(self):
        found, _ = self.tree(**{
            "low/mod.rs": "//! Depends on: no other eval folder.\n#[cfg(test)]\nmod probe;\n",
            "low/probe.rs": "use crate::eval::top;\n",
            "low/tests.rs": "use crate::eval::mid;\n",
            "low/tests/x.rs": "use crate::eval::mid;\n",
            "low/f.rs": "#[cfg(test)]\nuse crate::eval::top;\n"
                        "#[cfg(test)]\nmod checks { use crate::eval::mid; }\n"
                        "// use crate::eval::mid;\nconst S: &str = \"crate::eval::mid::B\";\n"})
        self.assertEqual(found, [])

    def test_missing_paragraph_and_cycles_outside_the_group(self):
        found, _ = self.tree(low="//! No dependency line.\n",
                             **{"low/g.rs": "use crate::eval::top;\n"})
        self.assertEqual(found, ["missing-depends-on low", "cycle low mid top"])

    def test_cycles_inside_the_recursion_group_are_allowed(self):
        sources = {"mod.rs": "", "vm/mod.rs": "//! Depends on: session.\n",
                   "vm/a.rs": "use crate::eval::session;\n",
                   "session/mod.rs": "//! Depends on: vm.\n",
                   "session/b.rs": "use crate::eval::vm;\n"}
        found, _ = collect(sources)
        self.assertEqual(found, [])
        self.assertEqual(sorted(map(sorted, strongly_connected({"a": {"b"}, "b": {"a"}}))),
                         [["a", "b"]])

    def test_evaluator_tree(self):
        found, _ = scan()
        self.assertEqual(found, [], "\n".join(found))


if __name__ == "__main__":
    if "--edges" in sys.argv:
        _, edges = scan()
        for folder in sorted(edges):
            print(f"{folder} -> {', '.join(sorted(edges[folder])) or '(none)'}")
    else:
        unittest.main()
