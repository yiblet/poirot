"""Check the evaluator module layout rules (Python 3.11+).

Run `python3 tools/test_module_layout.py` to check `src/eval/`; the check fails on
any violation.

Rules, applied to every `.rs` file under `src/eval/`:

- `impl-outside-definition`: an `impl Type` or `impl Trait for Type` block sits in a
  file other than the one defining `Type`. A type counts as defined in a file when a
  `struct`, `enum`, `union` or `type` item names it there. `impl Trait for Type` may
  also sit in the file defining `Trait`. Only types defined under `src/eval/` are
  checked; types from other modules or crates, generic parameters and types produced
  by macros are ignored. Item-level `macro_rules!` invocations count too: when an arm
  of the macro expands to `impl .. $name` (or `impl .. Type`), the invocation is
  matched to the arm by the literal tokens before `$name` in its matcher, and the type
  it names must be defined in the invoking file (or the impl's trait there). Macros
  generating impls through nested macros or repetitions are not followed.
- `mod-rs-content`: a `mod.rs` holds an item other than a `mod name;` declaration or
  a `use` declaration. Attributes and comments are allowed; inline `mod name { .. }`
  bodies are not.
- `mod-after-item`: a `mod name;` declaration follows any other item (including `use`)
  in the same file or inline module. Inline `mod name { .. }` bodies are not
  declarations and may appear anywhere outside `mod.rs`.
- `file-beside-dir`: both `<name>.rs` and a `<name>/` directory exist.
- `missing-blank-line`: two adjacent items in a file, inline module, `impl` or `trait`
  body have no blank line between the end of the first and the start of the second.
  Doc comments, comments and attributes belong to the item they precede. Runs of
  `use` declarations and runs of `mod name;` declarations may stay contiguous.
- `mod-rs-passthrough`: a `mod.rs` holds a private `use`. A `mod.rs` contains only
  declarations, so a private import can only serve descendants through `super::`
  paths or globs; descendants must import from the defining module instead.
- `enclosing-reexport`: a path names an item through a `use` in the `mod.rs` of a
  module that encloses the file (including the eval root), or globs such a module.
  Re-exports form a folder's API for callers outside it; code inside the folder
  imports from the defining module. Only `crate::`, `super::` and `self::` paths in
  `use` declarations and code are resolved.
- `multi-type-file`: a file defines more than one visible type, i.e. a `struct`,
  `enum`, `union`, `trait` or `type` alias with any `pub`/`pub(..)` visibility at
  the file's top level. A file has one primary type; extra types stay only when
  private to the file and used only by that primary type (left to review, since the
  scan does not resolve uses). `#[cfg(test)]` items, `tests.rs` files, `tests/`
  folders and the `adapters/` folder are exempt.
- `file-type-name`: a file whose single visible type is `Name` is not named
  `name.rs` (snake case of the type). Files named by role because the type shares its
  folder's name (`clippy::module_inception`) are listed in `ROLE_NAMED_FILES`. Files
  without a visible type hold free functions and are not checked. Same exemptions as
  `multi-type-file`.
- `duplicate-type-name`: two files define a top-level type (visible or private) with
  the same name, so the name alone does not find its file. Same exemptions as
  `multi-type-file`.
- `inline-test-module`: a `#[cfg(test)] mod name { .. }` body sits in a source file.
  Tests belong in the folder's `tests.rs` or `tests/` module. `tests.rs` files,
  `tests/` folders and the `adapters/` folder are exempt.
- `cross-folder-super`: a `super::` path in a `use` declaration or in code climbs to
  the eval root (or beyond) from inside a top-level folder. `super::` stays within
  one top-level folder; paths into another folder use `crate::eval::<folder>::..`.
  Chains are resolved against the file's module path and enclosing inline modules.

This is a lightweight syntactic scan: comments and string/char literals are masked
before items are found by brace depth. Items inside function bodies are not
inspected; macro bodies are inspected only for the impls described above.
"""
from collections import Counter
from dataclasses import dataclass, field
from itertools import dropwhile
from pathlib import Path
import bisect
import re
import sys
import unittest

from test_coercion_architecture import mask_literals_and_comments

ROOT = Path(__file__).resolve().parents[1]
SCAN_ROOT = Path("src/eval")
RULES = (
    "impl-outside-definition",
    "mod-rs-content",
    "mod-after-item",
    "file-beside-dir",
    "missing-blank-line",
    "mod-rs-passthrough",
    "enclosing-reexport",
    "multi-type-file",
    "file-type-name",
    "duplicate-type-name",
    "inline-test-module",
    "cross-folder-super",
)
KEYWORDS = {"fn", "struct", "enum", "union", "trait", "impl", "mod", "type", "const",
            "static", "use", "macro_rules"}
CONTAINERS = {"impl", "trait", "mod"}
TYPE_KINDS = {"struct", "enum", "union", "type"}
DEFINITION_KINDS = TYPE_KINDS | {"trait"}
TEST_ATTRIBUTE = re.compile(r"cfg\s*\(\s*test\s*\)")
EXEMPT_FOLDERS = {"adapters"}
# Files named by role because their type shares the enclosing folder's name.
ROLE_NAMED_FILES = {
    "overlay/incremental.rs": "Overlay",
    "session/composition.rs": "Session",
    "vm/bytecode_vm.rs": "Vm",
}
OPEN = "([{"
CLOSE = ")]}"


@dataclass
class Item:
    """One item found in a file, inline module, impl or trait body."""

    kind: str
    name: str
    start: int
    end: int
    body: tuple[int, int] | None = None
    children: list["Item"] = field(default_factory=list)
    impl_type: str | None = None
    impl_trait: str | None = None
    impl_params: frozenset[str] = frozenset()
    visible: bool = False
    test_only: bool = False

    @property
    def label(self):
        """Return a line-independent description such as `fn name` or `impl Type`."""
        if self.kind in {"impl", "macro", "other"}:
            return self.name
        return f"{self.kind} {self.name}"


def matching(code, opening, limit):
    """Return the index of the delimiter closing `code[opening]`, or `limit`."""
    depth = 0
    for index in range(opening, limit):
        char = code[index]
        if char in OPEN:
            depth += 1
        elif char in CLOSE:
            depth -= 1
            if depth == 0:
                return index
    return limit


def skip_space(code, index, limit):
    """Advance past whitespace."""
    while index < limit and code[index].isspace():
        index += 1
    return index


def angle_close(text, start):
    """Return the index after the `>` closing the `<` at `text[start]`."""
    depth = 0
    for index in range(start, len(text)):
        if text[index] == "<":
            depth += 1
        elif text[index] == ">" and text[index - 1] != "-":
            depth -= 1
            if depth == 0:
                return index + 1
    return len(text)


def split_top(text, separator):
    """Split on `separator` occurrences outside angle, paren and bracket nesting."""
    parts, depth, last = [], 0, 0
    for index, char in enumerate(text):
        if char in "<([":
            depth += 1
        elif char in ">)]" and not (char == ">" and text[index - 1] == "-"):
            depth -= 1
        elif depth == 0 and text.startswith(separator, index):
            parts.append(text[last:index])
            last = index + len(separator)
    parts.append(text[last:])
    return parts


def classify(header):
    """Return `(kind, name, keyword_end)` for an item header, or `('other', text, 0)`."""
    macro = re.match(r"\s*((?:\w+\s*::\s*)*\w+)\s*!", header)
    if macro and macro[1] != "macro_rules":
        return "macro", "".join(macro[1].split()) + "!", macro.end()
    match = re.match(r"\s*pub\s*(?:\([^)]*\))?", header)
    position = match.end() if match else 0
    tokens = list(re.finditer(r"[A-Za-z_]\w*|!", header[position:]))
    for index, token in enumerate(tokens):
        word = token[0]
        following = tokens[index + 1][0] if index + 1 < len(tokens) else ""
        if word in {"default", "async", "unsafe", "auto"}:
            continue
        if word == "const" and following in {"fn", "unsafe", "async", "extern"}:
            continue
        if word == "extern":
            if following == "crate":
                return "use", "extern crate", position + token.end()
            if following in {"fn", "unsafe"}:
                continue
            return "extern", "extern", position + token.end()
        if word in KEYWORDS:
            name = following if word != "macro_rules" else (
                tokens[index + 2][0] if index + 2 < len(tokens) else "")
            return word, name, position + token.end()
        break
    return "other", " ".join(header.split())[:40], 0


def parse_impl(item, header):
    """Fill the trait, self type and generic parameter names of an impl item."""
    text = " ".join(header.split())
    params = set()
    if text.startswith("<"):
        close = angle_close(text, 0)
        for param in split_top(text[1:close - 1], ","):
            words = re.findall(r"'?\w+", param)
            if words and words[0] == "const" and len(words) > 1:
                params.add(words[1])
            elif words and not words[0].startswith("'"):
                params.add(words[0])
        text = text[close:]
    text = re.split(r"\bwhere\b", text)[0].strip()
    halves = [part.strip() for part in re.split(r"\bfor\b(?!\s*<)", text)]
    if len(halves) == 2 and len(split_top(text, " for ")) == 2:
        trait, self_type = halves
    else:
        trait, self_type = None, text
    self_type = re.sub(r"^(?:&\s*(?:'\w+\s*)?(?:mut\s+)?|dyn\s+)+", "", self_type)
    path = re.match(r"[\w:]+", self_type)
    base = path[0].split("::")[-1] if path else self_type
    item.impl_type = base
    item.impl_params = frozenset(params)
    if trait is not None:
        trait = re.sub(r"'\w+\s*,?\s*", "", trait).replace("<>", "")
        item.impl_trait = trait
        item.name = f"impl {trait} for {base}"
    else:
        item.name = f"impl {base}"


def parse_items(code, start, limit):
    """Parse the items between `start` and `limit` of masked code."""
    items = []
    index = start
    while True:
        index = skip_space(code, index, limit)
        if index >= limit:
            return items
        if code.startswith("#!", index):
            bracket = skip_space(code, index + 2, limit)
            index = matching(code, bracket, limit) + 1
            continue
        item_start = index
        while index < limit and code[index] == "#":
            bracket = skip_space(code, index + 1, limit)
            index = skip_space(code, matching(code, bracket, limit) + 1, limit)
        head = index
        while index < limit and code[index] not in ";{" + CLOSE:
            index = matching(code, index, limit) + 1 if code[index] in "([" else index + 1
        if index >= limit or code[index] in CLOSE:
            return items
        kind, name, keyword_end = classify(code[head:index])
        body = None
        if code[index] == "{":
            close = matching(code, index, limit)
            body = (index + 1, close)
            index = close
            if kind in {"const", "static", "type", "use", "other"}:
                while index < limit and code[index] != ";":
                    index = matching(code, index, limit) + 1 if code[index] in OPEN else index + 1
        if kind == "use":
            name = " ".join(code[head + keyword_end:index].split("{")[0].split())
            name = name + "{..}" if "{" in code[head:index] else name
        item = Item(kind, name, item_start, min(index, limit - 1) + 1, body,
                    visible=re.match(r"\s*pub\b", code[head:index]) is not None,
                    test_only=TEST_ATTRIBUTE.search(code[item_start:head]) is not None)
        if kind == "impl":
            parse_impl(item, code[head + keyword_end:body[0] - 1 if body else index])
        if kind in CONTAINERS and body is not None:
            item.children = parse_items(code, body[0], body[1])
        items.append(item)
        index += 1


@dataclass
class SourceFile:
    """A scanned Rust file with masked code and its items."""

    path: str
    source: str
    code: str
    items: list[Item]
    line_starts: list[int]
    lines: list[str]

    @classmethod
    def parse(cls, path, source):
        """Mask literals and comments, then parse the top-level items."""
        code = mask_literals_and_comments(source)
        starts = [0] + [match.end() for match in re.finditer("\n", source)]
        return cls(path, source, code, parse_items(code, 0, len(code)), starts,
                   source.split("\n"))

    def line(self, offset):
        """Return the zero-based line containing `offset`."""
        return bisect.bisect_right(self.line_starts, offset) - 1

    def has_blank_line(self, after, before):
        """Report whether a blank source line lies strictly between two offsets."""
        return any(not self.lines[number].strip()
                   for number in range(self.line(after) + 1, self.line(before)))


def walk(items, scope=""):
    """Yield `(scope, items)` for every item list, including nested containers."""
    yield scope, items
    for item in items:
        if item.children:
            yield from walk(item.children, f"{scope}{item.label}/")


def is_declaration(item):
    """Report whether an item is a body-less `mod name;` declaration."""
    return item.kind == "mod" and item.body is None


def file_violations(file):
    """Return rule 2, 3 and 5 violations for one parsed file."""
    found = []
    if Path(file.path).name == "mod.rs":
        for item in file.items:
            if item.kind != "use" and not is_declaration(item):
                found.append(f"mod-rs-content {file.path} {item.label}")
    for scope, items in walk(file.items):
        seen_other = False
        for item in items:
            if is_declaration(item):
                if seen_other:
                    found.append(f"mod-after-item {file.path} {scope}{item.label}")
            else:
                seen_other = True
        for previous, current in zip(items, items[1:]):
            grouped = (previous.kind == current.kind == "use"
                       or is_declaration(previous) and is_declaration(current))
            if not grouped and not file.has_blank_line(previous.end - 1, current.start):
                found.append(f"missing-blank-line {file.path} {scope}{previous.label} -> {current.label}")
    return found


def is_type_exempt(file, root):
    """Report whether a file is exempt from the one-type-per-file naming rules."""
    parts = Path(file.path).relative_to(root).parts
    return parts[0] in EXEMPT_FOLDERS or parts[-1] == "tests.rs" or "tests" in parts[:-1]


def top_level_types(file):
    """Return the non-test top-level type and trait definitions of one file."""
    return [item for item in file.items if item.kind in DEFINITION_KINDS and not item.test_only]


def snake_case(name):
    """Convert a Rust type name such as `HTTPReadPlan` to `http_read_plan`."""
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", "_", name).lower()


def multi_type_violations(file, root):
    """Return the `multi-type-file` and `file-type-name` violations of one parsed file."""
    if is_type_exempt(file, root):
        return []
    visible = [item for item in top_level_types(file) if item.visible]
    if len(visible) > 1:
        return [f"multi-type-file {file.path} {', '.join(item.label for item in visible)}"]
    path = Path(file.path)
    if not visible or path.name == "mod.rs":
        return []
    name = visible[0].name
    relative = str(path.relative_to(root))
    if path.stem == snake_case(name) or ROLE_NAMED_FILES.get(relative) == name:
        return []
    return [f"file-type-name {file.path} {visible[0].label}"]


def trait_name(item):
    """Return the base name of an impl's trait (`fmt::Debug` -> `Debug`), or `None`."""
    if item.impl_trait is None:
        return None
    return re.split(r"\s*<", item.impl_trait.strip())[0].split("::")[-1].strip()


def inline_test_violations(file, root):
    """Return `inline-test-module` violations: `#[cfg(test)]` inline module bodies."""
    if is_type_exempt(file, root):
        return []
    return [f"inline-test-module {file.path} {scope}{item.label}"
            for scope, items in walk(file.items) for item in items
            if item.kind == "mod" and item.body is not None and item.test_only]


def duplicate_type_violations(files, root):
    """Return `duplicate-type-name` violations: one entry per extra defining file."""
    homes = {}
    for file in files:
        if not is_type_exempt(file, root):
            for item in top_level_types(file):
                homes.setdefault(item.name, []).append(file.path)
    return [f"duplicate-type-name {path} {name} (also {paths[0]})"
            for name, paths in homes.items() for path in paths[1:]]


USE = re.compile(r"(?<![\w:])(pub\s*(?:\([^)]*\))?\s*)?use\s([^;]*);")
PATH = re.compile(r"(?<![\w:])(?:crate|super|self)(?:\s*::\s*\w+)+")


def use_leaves(tree, prefix=()):
    """Expand a `use` tree into `(segments, glob)` leaves; aliases are dropped."""
    tree = re.sub(r"\s+as\s+\w+", "", " ".join(tree.split())).replace(" ", "")
    if tree.startswith("{"):
        return [leaf for part in split_top(tree[1:-1], ",") if part
                for leaf in use_leaves(part, prefix)]
    head, _, rest = tree.partition("::{")
    if rest:
        return use_leaves("{" + rest, prefix + tuple(head.split("::")))
    segments = prefix + tuple(tree.split("::"))
    if segments[-1] == "*":
        return [(segments[:-1], True)]
    return [(segments[:-1] if segments[-1] == "self" else segments, False)]


@dataclass
class ModuleFile:
    """A file's module path, inline module bodies and `use` declarations."""

    file: SourceFile
    module: tuple[str, ...]
    inline: list[tuple[int, int, str]]
    uses: list[tuple[int, int, str, str]]

    @classmethod
    def parse(cls, file, root):
        """Locate inline modules and `use` declarations in masked code."""
        relative = Path(file.path).relative_to(root).with_suffix("").parts
        module = relative[:-1] if relative[-1] == "mod" else relative
        inline = [(match.end() - 1, matching(file.code, match.end() - 1, len(file.code)), match[1])
                  for match in re.finditer(r"(?<!\w)mod\s+(\w+)\s*\{", file.code)]
        uses = [(match.start(), match.end(), (match[1] or "").strip(), match[2])
                for match in USE.finditer(file.code)]
        return cls(file, tuple(module), inline, uses)

    def scope(self, offset):
        """Return the module path of the innermost inline module around `offset`."""
        names = sorted((start, name) for start, end, name in self.inline if start < offset < end)
        return self.module + tuple(name for _, name in names)


def passthrough_violations(files, root):
    """Return `mod-rs-passthrough`, `enclosing-reexport` and `cross-folder-super` violations."""
    crate = tuple(Path(root).parts[1:] if Path(root).parts[0] == "src" else Path(root).parts)
    modules = [ModuleFile.parse(file, root) for file in files]
    children, exported, folders = {}, {}, set()
    for module in modules:
        names = children.setdefault(module.module, set())
        names.update(re.findall(r"(?<!\w)mod\s+(\w+)\s*;", module.file.code))
        for start, _, name in module.inline:
            children.setdefault(module.scope(start), set()).add(name)
        if Path(module.file.path).name == "mod.rs":
            folders.add(module.module)
            exported[module.module] = {
                segments[-1] for start, _, _, tree in module.uses if module.scope(start) == module.module
                for segments, glob in use_leaves(tree) if not glob}
    found = []
    for module in modules:
        path = module.file.path
        if module.module in folders:
            for start, _, visibility, tree in module.uses:
                if module.scope(start) == module.module and not visibility:
                    found.append(f"mod-rs-passthrough {path} use {' '.join(tree.split())}")
        leaves = [(start, segments, glob) for start, _, _, tree in module.uses
                  for segments, glob in use_leaves(tree)]
        spans = [(start, end) for start, end, _, _ in module.uses]
        leaves += [(match.start(), tuple(re.findall(r"\w+", match[0])), False)
                   for match in PATH.finditer(module.file.code)
                   if not any(start <= match.start() < end for start, end in spans)]
        for start, segments, glob in leaves:
            scope = module.scope(start)
            climbs = len(segments) - len(tuple(dropwhile(lambda part: part == "super", segments)))
            if scope and climbs >= len(scope):
                found.append(f"cross-folder-super {path} {'::'.join(segments)}")
            absolute = resolve_path(segments, scope, crate)
            if absolute is None:
                continue
            for index in range(len(absolute) + 1):
                owner, name = absolute[:index], absolute[index] if index < len(absolute) else None
                if owner != scope[:len(owner)]:
                    break
                if name is None:
                    if glob and owner in folders and owner != scope:
                        found.append(f"enclosing-reexport {path} {'::'.join(segments)}::*")
                    break
                if owner not in children or name in children[owner]:
                    continue
                if name in exported.get(owner, ()):
                    found.append(f"enclosing-reexport {path} {'::'.join(segments)}")
                break
    return found


def resolve_path(segments, scope, crate):
    """Return the module path below the scanned root for a `crate`/`super`/`self` path."""
    if segments[0] == "crate":
        if segments[1:len(crate) + 1] != crate:
            return None
        return segments[len(crate) + 1:]
    if segments[0] == "self":
        return scope + segments[1:]
    if segments[0] == "super":
        count = len(segments) - len(tuple(dropwhile(lambda part: part == "super", segments)))
        if count > len(scope):
            return None
        return scope[:len(scope) - count] + segments[count:]
    return None


TOKEN = re.compile(r"\$\w+\s*:\s*\w+|\$\w+|\w+|=>|::|\S")


@dataclass
class ImplTemplate:
    """An item-level `impl` in a macro arm and the matcher tokens that locate its type."""

    item: Item
    variable: str | None
    prefix: list[str]


def tokens(text):
    """Split masked code into macro tokens, keeping `$name:fragment` together."""
    return [re.sub(r"\s+", "", token) for token in TOKEN.findall(text)]


def macro_arms(code, body):
    """Yield `(matcher, transcriber)` text pairs of a `macro_rules!` body."""
    index = body[0]
    while True:
        index = skip_space(code, index, body[1])
        if index >= body[1] or code[index] not in OPEN:
            return
        matcher_end = matching(code, index, body[1])
        arrow = code.find("=>", matcher_end, body[1])
        opening = skip_space(code, arrow + 2, body[1]) if arrow >= 0 else body[1]
        if opening >= body[1] or code[opening] not in OPEN:
            return
        closing = matching(code, opening, body[1])
        yield code[index + 1:matcher_end], code[opening + 1:closing]
        index = closing + 1
        index = skip_space(code, index, body[1])
        if index < body[1] and code[index] == ";":
            index += 1


def transcriber_impls(transcriber):
    """Return the item-level `impl` blocks of a macro transcriber as parsed items."""
    found = []
    for match in re.finditer(r"\bimpl\b", transcriber):
        before = transcriber[:match.start()].rstrip()
        if before and before[-1] not in "{};]":
            continue
        header_end = transcriber.find("{", match.end())
        if header_end < 0:
            continue
        item = Item("impl", "", 0, 0)
        parse_impl(item, transcriber[match.end():header_end])
        found.append(item)
    return found


def macro_impls(files):
    """Map macro names to the impl templates their `macro_rules!` arms expand to."""
    templates = {}
    for file in files:
        for _, items in walk(file.items):
            for item in items:
                if item.kind != "macro_rules" or item.body is None:
                    continue
                for matcher, transcriber in macro_arms(file.code, item.body):
                    matcher_tokens = tokens(matcher)
                    for impl in transcriber_impls(transcriber):
                        variable = impl.impl_type[1:] if impl.impl_type.startswith("$") else None
                        prefix = []
                        if variable is not None:
                            position = next((index for index, token in enumerate(matcher_tokens)
                                             if re.fullmatch(rf"\${variable}(?::\w+)?", token)), None)
                            if position is None:
                                continue
                            literal = list(dropwhile(lambda token: not token.startswith("$"),
                                                     reversed(matcher_tokens[:position])))
                            prefix = matcher_tokens[len(literal):position]
                        templates.setdefault(item.name, []).append(ImplTemplate(impl, variable, prefix))
    return templates


def invocation_impls(file, item, templates):
    """Return the impls an item-level macro invocation generates, with resolved self types."""
    text = file.code[item.start:item.end]
    call = re.search(r"((?:\w+\s*::\s*)*\w+)\s*!", text)
    if call is None:
        return []
    arguments = tokens(text[call.end():])[1:-1]
    found = []
    for template in templates.get(item.name.rstrip("!").split("::")[-1], []):
        if template.variable is None:
            found.append(Item("impl", f"{item.name} {template.item.name}", 0, 0,
                              impl_trait=template.item.impl_trait, impl_type=template.item.impl_type))
            continue
        width = len(template.prefix)
        starts = [index + width for index in range(len(arguments) - width + 1)
                  if arguments[index:index + width] == template.prefix]
        for start in starts[:1]:
            path = list(dropwhile(lambda token: token == "::", arguments[start:]))
            segments = []
            while path and re.fullmatch(r"\w+", path[0]):
                segments.append(path.pop(0))
                if not path or path[0] != "::":
                    break
                path.pop(0)
            if segments:
                generated = Item("impl", "", 0, 0, impl_trait=template.item.impl_trait,
                                 impl_type=segments[-1])
                trait = f"{template.item.impl_trait} for " if template.item.impl_trait else ""
                generated.name = f"{item.name} impl {trait}{segments[-1]}"
                found.append(generated)
    return found


def collect(sources, root=str(SCAN_ROOT)):
    """Return all violations for `{relative_path: source}` files under `root`."""
    files = [SourceFile.parse(f"{root}/{path}", source) for path, source in sorted(sources.items())]
    found = []
    definitions, traits = {}, {}
    for file in files:
        found.extend(file_violations(file))
        found.extend(multi_type_violations(file, root))
        found.extend(inline_test_violations(file, root))
        for _, items in walk(file.items):
            for item in items:
                if item.kind in TYPE_KINDS:
                    definitions.setdefault(item.name, set()).add(file.path)
                elif item.kind == "trait":
                    traits.setdefault(item.name, set()).add(file.path)
    templates = macro_impls(files)
    for file in files:
        for _, items in walk(file.items):
            for item in items:
                if item.kind == "macro":
                    impls = invocation_impls(file, item, templates)
                elif item.kind == "impl" and item.impl_type not in item.impl_params:
                    impls = [item]
                else:
                    impls = []
                for impl in impls:
                    homes = definitions.get(impl.impl_type, set())
                    if homes and file.path not in homes and file.path not in traits.get(trait_name(impl), ()):
                        found.append(f"impl-outside-definition {file.path} {impl.name}")
    found.extend(passthrough_violations(files, root))
    found.extend(duplicate_type_violations(files, root))
    paths = set(sources)
    directories = {str(Path(path).parent) for path in paths}
    directories |= {str(parent) for path in paths for parent in Path(path).parents}
    for path in sorted(paths):
        if path.endswith(".rs") and path[:-3] in directories:
            found.append(f"file-beside-dir {root}/{path}")
    return sorted(found)


def scan(repository=ROOT, root=SCAN_ROOT):
    """Return all violations for the Rust files under `repository/root`."""
    base = repository / root
    sources = {str(path.relative_to(base)): path.read_text() for path in base.rglob("*.rs")}
    found = collect(sources, str(root))
    if (repository / root).with_suffix(".rs").exists():
        found.append(f"file-beside-dir {root}.rs")
    return sorted(found)


def counts(entries):
    """Format per-rule counts."""
    tally = Counter(entry.split(" ", 1)[0] for entry in entries)
    return ", ".join(f"{rule}={tally[rule]}" for rule in RULES)


class ModuleLayoutTests(unittest.TestCase):
    """Check the repository and the scanner rules."""

    def test_eval_layout_has_no_violations(self):
        found = scan()
        print(f"\nmodule layout violations: {counts(found)}", file=sys.stderr)
        self.assertEqual(found, [], "\nviolations (fix them):\n" + "\n".join(found))

    def rules(self, sources):
        return [entry.split(" ", 1)[0] for entry in collect(sources, "r")]

    def test_impl_outside_definition(self):
        sources = {"a.rs": "pub struct A<'x> { x: &'x u8 }",
                   "b.rs": "impl<'x> A<'x> {\n    fn f(&self) {}\n}"}
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/b.rs impl A"])
        sources["b.rs"] = "impl std::fmt::Debug for crate::r::a::A<'_> {}"
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/b.rs impl std::fmt::Debug for A"])

    def test_impl_beside_definition_or_foreign_type_passes(self):
        self.assertEqual(self.rules({"a.rs": "struct A;\n\nimpl A {}\n\nimpl Clone for A {}"}), [])
        self.assertEqual(self.rules({"a.rs": "enum A {}", "b.rs": "impl Clone for Workbook {}"}), [])
        self.assertEqual(self.rules({"a.rs": "struct T;", "b.rs": "impl<T: Copy> Show for T {}"}), [])
        self.assertEqual(self.rules({"a.rs": "type A = u8;\n\nimpl Trait for Vec<A> {}"}), [])

    def test_impl_of_local_trait_in_trait_file_passes(self):
        sources = {"a.rs": "pub struct A;", "t.rs": "trait Show {}\n\nimpl Show for crate::r::a::A {}\n",
                   "u.rs": "trait Other {}\n\nimpl<T> crate::r::t::Show<T> for A {}\n"}
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/u.rs impl crate::r::t::Show<T> for A"])
        sources["u.rs"] = "trait Other {}\n\nimpl Other for A {}\n\nimpl A {}\n"
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/u.rs impl A"])

    def test_macro_generated_impl_outside_definition(self):
        macro = ("macro_rules! helper_map {\n    (total $family:ident => $target:ty;) => {\n"
                 "        impl $family {\n            fn helper(self) {}\n        }\n    };\n"
                 "    (trait $name:ident) => {\n        impl Show for $name {}\n    };\n}\n")
        sources = {"a.rs": "pub enum A {}", "m.rs": macro + "\nhelper_map! {\n    total A => x::A;\n}\n"}
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/m.rs helper_map! impl A"])
        sources["m.rs"] = macro + "\nhelper_map!(trait crate::r::a::A);\n"
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/m.rs helper_map! impl Show for A"])
        sources["m.rs"] = "macro_rules! fixed {\n    () => {\n        impl A {}\n    };\n}\n\nfixed!();\n"
        self.assertEqual(collect(sources, "r"), ["impl-outside-definition r/m.rs fixed! impl A"])

    def test_macro_generated_impl_in_definition_file_passes(self):
        macro = ("macro_rules! helper_map {\n    (total $family:ident => $target:ty;) => {\n"
                 "        impl $family {}\n    };\n}\n")
        sources = {"m.rs": macro, "a.rs": "pub enum A {}\n\nhelper_map! {\n    total A => x::B;\n}\n",
                   "b.rs": "pub enum B {}\n\nimpl Show for B {}\n",
                   "t.rs": "trait Show {}\n\nmacro_rules! show {\n    ($t:ty) => {\n        impl Show for $t {}\n    };\n}\n\nshow!(B);\n"}
        self.assertEqual(self.rules(sources), [])

    def test_macro_expanding_inside_an_impl_body_passes(self):
        macro = ("macro_rules! methods {\n    () => {\n        fn set(&mut self, value: impl Into<A>) {}\n"
                 "    };\n}\n\npub(super) use methods;\n")
        sources = {"a.rs": "pub struct A;", "m.rs": macro,
                   "b.rs": "pub struct B;\n\nimpl B {\n    crate::r::m::methods!();\n}\n"}
        self.assertEqual(self.rules(sources), [])

    def test_inline_test_modules(self):
        source = "struct A;\n\n#[cfg(test)]\nmod tests {\n    #[cfg(test)]\n    mod nested {}\n}\n\nmod inner {}\n"
        self.assertEqual(collect({"m/a.rs": source}, "r"), [
            "inline-test-module r/m/a.rs mod tests",
            "inline-test-module r/m/a.rs mod tests/mod nested",
        ])
        allowed = {"m/tests.rs": source, "n/tests/a.rs": source, "adapters/x/a.rs": source,
                   "m/b.rs": "#[cfg(test)]\nmod tests;\n\nstruct B;\n"}
        self.assertEqual(self.rules(allowed), [])

    def test_mod_rs_allows_only_declarations(self):
        good = "//! Purpose.\n#![allow(x)]\n\n/// Doc.\n#[cfg(test)]\nmod tests;\npub(crate) mod a;\n\npub use a::B;\npub(crate) use a::{C, D};\n"
        self.assertEqual(self.rules({"m/mod.rs": good}), [])
        bad = "mod a;\n\nfn f() {}\n\n#[cfg(test)]\nmod tests {\n}\n"
        self.assertEqual(collect({"m/mod.rs": bad}, "r"),
                         ["inline-test-module r/m/mod.rs mod tests", "mod-rs-content r/m/mod.rs fn f",
                          "mod-rs-content r/m/mod.rs mod tests"])
        self.assertEqual(self.rules({"m/other.rs": "mod a;\n\nfn f() {}\n"}), [])

    def test_mod_declarations_come_first(self):
        self.assertEqual(collect({"a.rs": "use x::y;\n\nmod b;\n"}, "r"), ["mod-after-item r/a.rs mod b"])
        self.assertEqual(collect({"a.rs": "fn f() {}\n\nmod inner {\n    fn g() {}\n\n    mod c;\n}\n"}, "r"),
                         ["mod-after-item r/a.rs mod inner/mod c"])
        self.assertEqual(self.rules({"a.rs": "#![allow(x)]\n//! Doc.\nmod b;\nmod c;\n\nuse x;\n\nmod inner {}\n"}), [])

    def test_file_beside_directory(self):
        self.assertEqual(collect({"a.rs": "", "a/b.rs": ""}, "r"), ["file-beside-dir r/a.rs"])
        self.assertEqual(self.rules({"a/mod.rs": "", "a/b.rs": "", "c.rs": ""}), [])

    def test_missing_blank_line_between_items(self):
        source = "struct A;\n/// Doc.\nfn f() {}\n\nimpl A {\n    fn g() {}\n    #[inline]\n    fn h() {}\n}\nconst X: u8 = 1; const Y: u8 = 2;\n"
        self.assertEqual(collect({"a.rs": source}, "r"), [
            "missing-blank-line r/a.rs const X -> const Y",
            "missing-blank-line r/a.rs impl A -> const X",
            "missing-blank-line r/a.rs impl A/fn g -> fn h",
            "missing-blank-line r/a.rs struct A -> fn f",
        ])

    def test_blank_lines_and_grouped_declarations_pass(self):
        source = ("mod a;\nmod b;\n\nuse x;\nuse y::{z, w};\n\n/// Doc.\n#[derive(Debug)]\nstruct A {\n    f: u8,\n}\n\n"
                  "impl A {\n    const C: u8 = 1;\n\n    fn g() {\n        let x = 1;\n        let y = 2;\n    }\n}\n\n"
                  "macro_rules! m {\n    () => {};\n}\n\nm!();\n")
        self.assertEqual(self.rules({"a.rs": source}), [])
        self.assertEqual(collect({"a.rs": "a::m!();\nfn f() {}\n"}, "r"), ["missing-blank-line r/a.rs a::m! -> fn f"])

    def test_comments_and_literals_do_not_create_items(self):
        source = ('/// fn fake() {}\nfn f() {\n    let s = "}\\n\\nfn x() {";\n    let r = r#"{"#;\n    let c = \'{\';\n}\n'
                  "// impl B {}\n\n/* mod z;\n*/\nfn g<'a>(_: &'a str) {}\n")
        self.assertEqual(self.rules({"a.rs": source, "b.rs": "struct B;"}), [])

    def test_mod_rs_private_use_is_a_passthrough(self):
        source = "mod a;\n\n#[cfg(test)]\nuse crate::model::CellValue;\npub use a::B;\npub(in crate::r) use a::{C, D};\n"
        self.assertEqual(collect({"m/mod.rs": source, "m/a.rs": ""}, "r"),
                         ["mod-rs-passthrough r/m/mod.rs use crate::model::CellValue"])
        self.assertEqual(self.rules({"m/b.rs": "use crate::model::CellValue;\n"}), [])

    def test_paths_through_enclosing_reexports(self):
        sources = {"mod.rs": "mod m;\nmod n;\n\npub use m::B;\n",
                   "m/mod.rs": "mod a;\nmod c;\n\npub(crate) use a::{A, f};\n",
                   "m/a.rs": "pub(crate) struct A;\n\npub(crate) fn f() {}\n",
                   "m/c.rs": "use super::A;\nuse crate::r::B;\n\nfn g() {\n    super::super::m::f();\n}\n",
                   "n.rs": "use crate::r::m::{A, a::f};\n\nfn g() {\n    super::m::f();\n}\n"}
        self.assertEqual(collect(sources, "src/r"), [
            "cross-folder-super src/r/m/c.rs super::super::m::f",
            "cross-folder-super src/r/n.rs super::m::f",
            "enclosing-reexport src/r/m/c.rs crate::r::B",
            "enclosing-reexport src/r/m/c.rs super::A",
            "enclosing-reexport src/r/m/c.rs super::super::m::f",
        ])
        sources["m/c.rs"] = "use super::a::A;\nuse crate::r::m::a::f;\n\n// super::A\nfn g() {}\n"
        sources["n.rs"] = "use crate::r::m::{A, a::f};\n\nfn g() {\n    crate::r::m::f();\n}\n"
        self.assertEqual(self.rules(sources), [])

    def test_globs_of_enclosing_modules(self):
        sources = {"m/mod.rs": "mod a;\n#[cfg(test)]\nmod tests;\n\npub(crate) use a::A;\n",
                   "m/a.rs": "pub(crate) struct A;\n\nmod inner {\n    use super::*;\n}\n",
                   "m/tests.rs": "use super::*;\n"}
        self.assertEqual(collect(sources, "r"), ["enclosing-reexport r/m/tests.rs super::*"])
        sources["m/tests.rs"] = "use super::a::A;\n"
        self.assertEqual(self.rules(sources), [])

    def test_multiple_visible_types_in_one_file(self):
        source = ("pub struct A;\n\npub(crate) enum B {}\n\npub(super) trait C {}\n\n"
                  "pub(in crate::r) type D = u8;\n\nimpl A {}\n")
        self.assertEqual(collect({"m/a.rs": source}, "r"),
                         ["multi-type-file r/m/a.rs struct A, enum B, trait C, type D"])

    def test_private_helpers_tests_and_adapters_are_allowed(self):
        source = ("pub struct A {\n    b: B,\n}\n\n/// Helper.\nstruct B;\n\nenum C {}\n\n"
                  "type D = u8;\n\n#[cfg(test)]\npub(super) struct E;\n\n"
                  "impl A {\n    pub type F = u8;\n}\n\nmod inner {\n    pub struct G;\n}\n")
        self.assertEqual(self.rules({"m/a.rs": source}), [])
        two = "pub struct A;\n\npub struct B;\n"
        self.assertEqual(self.rules({"adapters/x/a.rs": two, "m/tests.rs": two, "n/tests/a.rs": two}), [])
        self.assertEqual(self.rules({"m/a.rs": "pub struct A;\n\npub fn f() {}\n\npub const X: u8 = 1;\n"}), [])

    def test_file_named_after_its_visible_type(self):
        sources = {"m/read_plan.rs": "pub struct CellReadPlan;\n\nstruct Helper;\n",
                   "m/cell_read_plan.rs": "pub(crate) enum CellReadPlan {}\n",
                   "m/logic.rs": "struct Private;\n\npub fn f() {}\n",
                   "m/http_plan.rs": "pub trait HTTPPlan {}\n"}
        self.assertEqual([entry for entry in collect(sources, "r") if entry.startswith("file-type-name")],
                         ["file-type-name r/m/read_plan.rs struct CellReadPlan"])
        role = {"overlay/incremental.rs": "pub struct Overlay;\n", "adapters/x.rs": "pub struct Y;\n",
                "m/tests.rs": "pub struct Z;\n", "m/a.rs": "#[cfg(test)]\npub struct B;\n"}
        self.assertEqual(self.rules(role), [])
        self.assertEqual(self.rules({"vm/bytecode_vm.rs": "pub struct Machine;\n"}), ["file-type-name"])

    def test_duplicate_type_names_across_files(self):
        sources = {"a/source.rs": "pub struct Source;\n", "b/plan.rs": "pub struct Plan;\n\nstruct Source;\n",
                   "adapters/source.rs": "pub struct Source;\n", "c/tests.rs": "struct Source;\n"}
        self.assertEqual(collect(sources, "r"),
                         ["duplicate-type-name r/b/plan.rs Source (also r/a/source.rs)"])

    def test_super_paths_stay_within_a_top_level_folder(self):
        sources = {"m/mod.rs": "mod a;\nmod b;\n",
                   "m/a.rs": ("use super::b::B;\nuse super::super::n::C;\n\n"
                              "fn f() {\n    super::super::n::g();\n}\n\n"
                              "mod inner {\n    use super::super::b::B;\n    use super::super::super::n::C;\n}\n"),
                   "m/b.rs": "pub struct B;\n",
                   "n.rs": "pub struct C;\n\npub fn g() {}\n",
                   "m/tests/x.rs": "use super::super::a;\nuse crate::r::n::C;\n"}
        self.assertEqual([entry for entry in collect(sources, "src/r") if entry.startswith("cross-folder-super")], [
            "cross-folder-super src/r/m/a.rs super::super::n::C",
            "cross-folder-super src/r/m/a.rs super::super::n::g",
            "cross-folder-super src/r/m/a.rs super::super::super::n::C",
        ])
        self.assertEqual(self.rules({"m/mod.rs": "mod a;\nmod b;\n", "m/a.rs": "use super::b::B;\n",
                                     "m/b.rs": "pub struct B;\n", "x.rs": "fn f() {}\n"}), [])


if __name__ == "__main__":
    unittest.main()
