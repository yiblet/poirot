---
name: documentation
description: Write, edit, or review Poirot documentation and API wording for accuracy and coherence at sentence, paragraph, and document level, using established Rust library conventions. Use for READMEs, guides, rustdoc, examples, and repository-wide documentation audits.
---

# Documentation

Good documentation gives a developer enough accurate understanding to use the
library or work on its implementation. Every sentence, paragraph, and section
should contribute to that purpose. Follow the repository's `AGENTS.md`.

## Start from the reader's task

Establish who will read the document, what they already know, and what they need
to do or understand afterward. Choose content and detail from those needs rather
than from the implementation's structure or the author's recent work. A quick
start teaches a usable path; API reference explains a contract; architecture
explains responsibilities and relationships. They need different levels of detail.

Read the implementation and its uses before explaining behavior. Separate facts,
necessary constraints, design choices, and unresolved questions. If you cannot
explain a claim concretely, investigate it or state the uncertainty. Polished
wording cannot substitute for understanding.

Use [Calamine](https://docs.rs/calamine/) and [CSV](https://docs.rs/csv/) as style
references when useful. Compare material serving the same audience and purpose.
Learn from their direct descriptions, examples, and placement of constraints;
do not copy their wording or impose a fixed structure on every document.

## Make the explanation coherent at every scale

Review all sentences and paragraphs, including familiar-looking prose. Clear
individual words do not guarantee a clear explanation.

At the sentence level, make the subject, action, and relevant conditions clear.
Use concrete behavior and consistent names. A reader looking at a small passage
should be able to understand it using nearby context and the document's stated
prerequisites, without reconstructing an internal discussion. Make references
such as "this policy" unambiguous. Check that each claim is supported and gives
the reader useful information; remove empty qualifications and unexplained labels.

At the paragraph level, develop one main point. Establish what it concerns, connect
its claims, and explain consequences where they help the reader. A group of true
sentences can still fail to explain their relationship. Rewrite that relationship
rather than merely adding transition words. Split paragraphs with unrelated jobs;
combine fragments that only make sense together.

At the document level, build understanding in a useful order. Introduce necessary
ideas before relying on them, connect examples to the surrounding explanation,
and place constraints where they affect a decision. Keep terminology and claims
consistent across sections and related documents. Read the whole result after
local edits to find gaps, repetition, contradictions, and material that no longer
belongs. Do not repeat every definition in every sentence to achieve local clarity.

For each passage, consider whether the reader would be better served by removing
it, moving it, explaining it concretely, or changing the structure around it.
Shorter is better only when the necessary meaning remains.

## Make new concepts earn their place

Generally avoid introducing new concepts. Prefer established Rust, spreadsheet,
and domain vocabulary, or describe the behavior directly. A new name adds
something the reader must learn and maintainers must keep consistent. Do not
invent words or concepts to cover behavior you do not yet understand.

During review, evaluate every newly introduced concept, including those expressed
through ordinary words, API names, and type names. Establish the concrete behavior
or distinction it represents and a use where that distinction matters. Consider
whether established terminology or a direct explanation would suffice. Keep the
concept only when its value to this reader justifies learning the additional
vocabulary and relationships. Define a retained concept in familiar terms before
using it to explain something else.

Apply this reasoning to the explanation itself: a paragraph can invent an
unnecessary distinction without coining a word. Do not formalize incidental
implementation details merely because they have names in the code.

## Preserve contracts and evidence

Keep limitations concrete: identify the affected input, behavior, or error.
Distinguish supported behavior, known compatibility limits, and unverified
agreement with Excel. Preserve useful constraints when simplifying prose.

Keep required upstream credits, URLs, revisions, licenses, and descriptions of
reused material in nearby attribution comments or established provenance files.
Assess legal records for accuracy and completeness; preserve required notices.

When API review is in scope, apply the same reasoning to names and abstractions.
Flag names that obscure behavior, unexplained policy choices, redundant entry
points, and missing exports needed to configure public types. Preserve distinct,
justified use cases. Report design issues separately when only documentation
edits are authorized.

## Verify the result and describe its scope

For a review against a branch, verify the base and working-tree state. Distinguish
changed issues from existing ones; if there is no diff, say so. For an
all-documentation audit, inventory and read the tracked Markdown, rustdoc, and
examples, including component crates, development tools, and benchmarks. State
any material left unreviewed; do not certify all documentation from a sample.

Use small realistic examples through the intended public API. Include enough
setup to run them, or clearly introduce fragments. Verify expected results from
documented behavior or independent calculations. Render examples in their actual
destination: rustdoc's hidden `#` lines and `no_run` annotations are not handled
that way by GitHub Markdown. Check links, imports, and error handling as well.

For documentation changes, build the affected crates' docs and run their
doctests. Check standalone Markdown examples separately unless the repository
explicitly includes them in doctests. Follow the repository's required checks
before submitting changes. For a documentation coverage audit, also run:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
RUSTDOCFLAGS="-W missing-docs" cargo doc --workspace --no-deps
```

Attribute diagnostics to their crates and distinguish production APIs from
unpublished tooling. These commands do not check private-type coverage or prove
that prose is accurate, coherent, or useful.

Report findings with file locations and concrete reader impact. Explain the
underlying problem rather than only identifying an awkward word. State what was
examined and checked, and distinguish verified results from editorial judgment.
