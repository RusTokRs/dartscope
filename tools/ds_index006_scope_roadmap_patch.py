#!/usr/bin/env python3
from pathlib import Path

path = Path("docs/development/dartscope-library-plan.md")
text = path.read_text(encoding="utf-8")

section = """### DS-INDEX-006: Broader Reference And Scope Resolution

Status: planned. Priority: P1. Prerequisites: DS-INDEX-005, DS-PARSE-006.

Required work:
"""
section_replacement = """### DS-INDEX-006: Broader Reference And Scope Resolution

Status: in progress. Priority: P1. Prerequisites: DS-INDEX-005, DS-PARSE-006.

Progress (2026-07-19):

1. Added a parser-side lexical-shadowing guard for the existing opt-in `invocation_target` facts.
   Parameters, preceding block-local variables, import-prefix collisions, and members of the enclosing
   type no longer escape into top-level namespace resolution. A nested-block local stops shadowing
   after its closing brace, while declarations after an invocation do not retroactively shadow it.
2. Preserved the public reference kind, confidence, exact spans, deterministic ordering, enclosing
   symbol IDs, pure file/project output, and all non-shadowed namespace-resolution behavior.
3. Added parser negative fixtures and an index integration fixture proving that the index resolves
   only parser-produced facts and never reparses raw Dart source.

Findings and limits:

- Suppressed roots are deliberately omitted rather than fabricated as resolved local/member facts.
  Closure parameters, loop/catch and pattern bindings, inherited members, extension lookup, constructor
  selection, type inference, and general reads/writes remain follow-up work.
- Every future reference kind remains opt-in and requires an explicit compatibility contract plus exact
  span and nearby-shadowing fixtures before it can enter public output.

Required work:
"""

acceptance = """Acceptance:

- declaration/reference fixtures include nearby shadowing and false-positive negatives;
- every new reference kind is opt-in until its compatibility contract is documented;
- existing invocation-target reference output is unchanged;
- index code never reparses raw source.
"""
acceptance_replacement = acceptance + "\nSee `docs/development/reference-scope-resolution.md`.\n"

next_step = """Implement `DS-INDEX-006` next. The durable quality gate is now verified across dependency policy,
bounded fuzzing, deterministic properties, relative benchmark reporting, and a non-blocking macOS
portability signal. Broader references and lexical scope resolution are the next P1 prerequisite for
language-server work; `DS-COMPAT-001` remains research.
"""
next_replacement = """Continue `DS-INDEX-006` with explicit opt-in type-position and constructor reference facts. The
initial lexical-shadowing guard now prevents parameters, visible block locals, import-prefix collisions,
and same-type members from being sent to top-level namespace resolution while preserving existing
non-shadowed invocation output. The next slice must define typed reference kinds and exact-span negative
fixtures before index lookup; `DS-COMPAT-001` remains research.
"""

for old, new in (
    (section, section_replacement),
    (acceptance, acceptance_replacement),
    (next_step, next_replacement),
):
    if text.count(old) != 1:
        raise SystemExit(f"roadmap anchor did not match exactly once: {old[:100]!r}")
    text = text.replace(old, new)

path.write_text(text, encoding="utf-8")
