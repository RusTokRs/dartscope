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

## Explicit typed-reference slice

The second `DS-INDEX-006` slice adds two additive opt-in kinds without changing existing
`invocation_target` facts:

- `type_annotation` covers only the nominal roots of `extends`, `with`, `implements`, and supported
  extension/mixin `on` clauses. Generic arguments are not swept as references, declaration type
  parameters suppress matching unqualified roots, and dotted roots require a declared import prefix.
- `constructor_target` requires an explicit `new` or `const` keyword. The fact points at the
  constructor's type token, including an import prefix when present. Ordinary `Type()` and
  `Factory.create()` calls remain only `invocation_target` facts because syntax alone does not prove
  constructor selection.

Both kinds retain exact identifier spans and parser-provided enclosing symbol evidence. The index
resolves the resulting facts through the same namespace context and still never reparses source.

## Declaration type-position slice

The third `DS-INDEX-006` slice adds three additive opt-in kinds:

- `parameter_type` covers the nominal root of an explicitly typed callable parameter;
- `return_type` covers the nominal root before a supported function, method, getter, or operator;
- `variable_type` covers the nominal root of an explicitly typed top-level variable, field, or local
  already represented by the declaration inventory.

The scanner keeps only exact root tokens. Declared import prefixes produce high-confidence facts;
unprefixed project roots remain medium confidence. Visible type parameters, inferred declarations,
`this`/`super` formals, SDK roots, record syntax, and nested generic arguments are deliberately omitted.
Every emitted fact retains the declaration's parser-provided enclosing symbol evidence and exact span.
The index resolves these facts through the existing namespace context and still never reparses source.

## Compatibility boundary

- Existing non-shadowed invocation-target facts keep their kind, confidence, exact span, ordering,
  enclosing symbol ID, and namespace-resolution behavior.
- Pure file/project analysis and serialized invocation output are unchanged.
- The index receives parser-produced facts only and never scans raw Dart source.
- Suppressed roots are not claimed as resolved local/member references yet; they are deliberately
  omitted until typed lexical/member candidate models are introduced.

## Deferred scope

This slice does not claim analyzer-equivalent lexical semantics. Closure parameters, pattern
bindings, loop/catch bindings, inherited members, extension lookup, implicit constructor selection,
nested generic arguments, SDK/external namespaces, record and function-type internals, metadata
annotations, type inference, overload resolution, and general variable read/write references remain
explicit follow-up work. Each future reference kind requires its own documented opt-in compatibility
contract and negative fixtures before it can enter public output.
