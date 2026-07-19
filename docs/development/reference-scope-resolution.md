---
id: doc://docs/development/reference-scope-resolution.md
kind: development_policy
language: en
source_language: en
status: active
---

# Conservative Reference And Scope Resolution

DartScope's identifier-reference pipeline is opt-in. `analyze_file` and `analyze_project` keep their
existing declaration and invocation output, while `analyze_*_with_references` emits only syntactically
bounded facts that the index can resolve without reparsing source.

## Initial lexical-shadowing slice

The first `DS-INDEX-006` slice retains the existing `invocation_target` reference kind and adds a
conservative guard before an invocation root is sent to the top-level namespace resolver. A root is
not emitted as a top-level reference when parser evidence shows that it is shadowed by:

1. a parameter of the enclosing callable;
2. a preceding local variable whose containing braced block also contains the invocation; or
3. a field, method, getter, setter, or operator declared on the enclosing type.

The same guard applies before interpreting a dotted root as an import prefix. Therefore a local
parameter named like an import prefix does not become a high-confidence prefixed namespace query.
A local declared in a nested block stops shadowing after that block closes. Declarations after an
invocation do not retroactively shadow the earlier invocation.

## Compatibility boundary

- Existing non-shadowed invocation-target facts keep their kind, confidence, exact span, ordering,
  enclosing symbol ID, and namespace-resolution behavior.
- Pure file/project analysis and serialized invocation output are unchanged.
- The index receives parser-produced facts only and never scans raw Dart source.
- Suppressed roots are not claimed as resolved local/member references yet; they are deliberately
  omitted until typed lexical/member candidate models are introduced.

## Deferred scope

This slice does not claim analyzer-equivalent lexical semantics. Closure parameters, pattern
bindings, loop/catch bindings, inherited members, extension lookup, constructor selection, type
inference, overload resolution, and general variable read/write references remain explicit follow-up
work. Each future reference kind requires its own documented opt-in compatibility contract and
negative fixtures before it can enter public output.
