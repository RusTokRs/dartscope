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

## Lexical binding slice

The fourth `DS-INDEX-006` slice adds parser-produced lexical binding facts alongside the existing
opt-in references:

- `parameter` records ordinary callable parameter names, including named and optional parameters;
- `local_variable` reuses the declaration inventory's stable local symbol IDs;
- every binding carries an exact identifier `declaration_span`, its enclosing callable symbol ID, and
  an explicit half-open `scope_span` consumed by the index without source reparsing;
- parameter scope begins after the callable's closing `)`, covering constructor initializer lists and
  executable bodies;
- local scope initially began after the complete declaration statement and ends at the closing brace
  of the nearest containing block. The declaration-order slice below refines the start per declarator.

The index exposes deterministic most-specific binding selection. A nested local wins over a parameter
only while its explicit scope contains the query offset; after the block closes, the parameter becomes
visible again. Receiver formals, wildcards, pattern bindings, and analyzer-equivalent declaration-order
semantics remain omitted.

## Variable-read slice

The fifth `DS-INDEX-006` slice adds the opt-in `variable_read` reference kind. A fact is emitted only
for an unqualified identifier token backed by exactly one most-specific parser-produced lexical
binding interval at that byte offset. The fact retains an exact identifier span, high confidence, and
the innermost modeled callable symbol ID.

The parser deliberately omits tokens that are not proven reads, including declaration identifiers,
member suffixes, labels and named-argument keys, assignment targets, compound assignments, increments,
callable headers, explicit type positions, and illegal self or later-declarator accesses. Legal reads
of an earlier declarator from a later initializer are enabled by the declaration-order slice below.
Anonymous-closure, `for`, and `catch` regions were initially omitted and are enabled only after the
lexical-region slice supplies explicit bindings and scopes.

`resolve_project_variable_read_references` resolves these facts only through the `bindings` intervals
already carried by `DartProjectReferenceAnalysis`. Namespace resolution filters `variable_read` facts
rather than treating them as top-level symbol queries. Most-specific selection remains deterministic:
the smaller visible scope wins, then the later declaration; equal best ranks remain ambiguous.

## Variable-write slice

The sixth `DS-INDEX-006` slice adds the opt-in `variable_write` reference kind for the target token
of a plain `=` assignment. The target must be one unqualified identifier backed by exactly one
most-specific parser-produced lexical binding interval. The fact retains the exact identifier span,
high confidence, and the innermost modeled callable symbol ID. Assignment right-hand sides continue
to produce independent `variable_read` facts.

The plain-write collector deliberately omits compound assignments, prefix/postfix increments, member
and indexed targets, and destructuring. Writes inside a later declarator initializer are enabled only
when an earlier declarator's explicit interval already contains the target; self and later-declarator
targets remain suppressed. Closure and supported control-region writes require an exact visible binding.
Equality and arrow tokens are not assignments.

`resolve_project_variable_write_references` resolves write facts through the same parser-produced
`bindings` intervals as reads. Namespace resolution filters both lexical access kinds and never
reparses source.

## Compound-update slice

The seventh `DS-INDEX-006` slice classifies every supported unqualified compound-assignment or
prefix/postfix increment target as a paired `variable_read` and `variable_write` at the same exact
identifier span. Compound assignments cover `+=`, `-=`, `*=`, `/=`, `%=`, `~/=`, `<<=`, `>>=`,
`>>>=`, `&=`, `|=`, `^=`, and `??=`. Both facts retain high confidence and the same enclosing
callable evidence; a compound assignment's right-hand side continues to emit independent reads.

The paired facts resolve independently through the existing read and write index entry points and
must select the same most-specific lexical binding. Member/index targets, destructuring, and declaration
initializers remain omitted. No new serialized kind is introduced: combined semantics are represented
by the deterministic fact pair.

## Closure and control binding slice

The eighth `DS-INDEX-006` slice adds parser-produced bindings for supported parenthesized
anonymous-closure parameters, braced single-declarator classic and `for-in` declarations, and
one- or two-name `catch` parameters. The public binding kinds remain `parameter` and
`local_variable`; stable symbol IDs retain `closure_parameter`, `for_variable`, or `catch_parameter`
origin plus the declaration byte offset.

Closure scope begins after `=>` or the opening body brace. A classic-loop declaration becomes visible
after the first semicolon and remains visible through its body; a `for-in` declaration is visible only
inside the following braced body. Catch parameters are visible only in the catch block. Supported region
headers are suppressed as lexical access positions, while iterable expressions, classic-loop
conditions and updates, and executable bodies emit binding-backed reads and writes normally.

Pattern and multi-declarator declarations, single-statement or collection control-flow elements,
unparenthesized/receiver/pattern/function-type closure parameters, and malformed regions remain fully
deferred. Existing-variable `for-in` targets are enabled only by the later focused assignment slice.
Invocation roots inside supported scopes are filtered by the same parser-produced binding intervals
before namespace resolution.

## Initializer and declaration-order slice

The ninth `DS-INDEX-006` slice refines each ordinary block-local binding interval independently. A
declarator with an explicit initializer becomes reference-eligible immediately after the end of that
initializer; a declarator without an initializer becomes eligible immediately after its identifier.
The scope still ends at the nearest containing block. This implements the normative legal case
`var first = seed, second = first;` without introducing a new binding or reference kind.

Reads, plain writes, paired compound/increment accesses, and invocation-root filtering inside later
initializers reuse the existing collectors and index entry points. A self-reference remains suppressed
until its initializer ends, and a reference to a later declarator in the same statement is also
suppressed rather than escaping to an outer parameter or namespace declaration. The index continues to
resolve only parser-produced intervals and never reparses source.

This focused slice does not retroactively shadow references in earlier statements with a later local
declaration, and it does not perform definite-assignment or flow analysis. Those positions preserve the
existing compatibility contract until a diagnostic-bearing pre-declaration slice is designed.

## Existing-variable for-in slice

The tenth `DS-INDEX-006` slice supports a braced `for-in` header whose left side is one unqualified
existing lexical variable. The header target emits one high-confidence `variable_write` at the exact
identifier span and creates no new lexical binding. The iterable expression is evaluated independently,
so its binding-backed reads remain visible before the per-iteration assignment.

Body reads, plain writes, combined updates, and invocation-root filtering reuse the existing visible
parameter or local interval. The target and body therefore resolve through the existing read/write index
entry points without source reparsing. A declared `for-in` variable keeps its separate loop-local binding
and does not emit an assignment-target write fact.

Pattern, member/index, wildcard, multi-target, malformed, single-statement, and collection `for-in`
forms remain deferred. This slice records syntax-proven assignment access only; it does not validate
mutability, element types, definite assignment, or flow reachability.

## Compatibility boundary

- Existing non-shadowed invocation-target facts keep their kind, confidence, exact span, ordering,
  enclosing symbol ID, and namespace-resolution behavior.
- Pure file/project analysis and serialized invocation output are unchanged.
- Existing reference fields and kinds remain unchanged; lexical bindings are additive fields on the
  already opt-in reference-analysis models and default to an empty list when deserializing older JSON.
- `variable_read` and `variable_write` are additive within the opt-in reference stream and are handled
  by separate lexical resolution entry points; existing namespace resolution continues to return
  namespace facts only.
- The index receives parser-produced facts only and never scans raw Dart source.
- Suppressed roots are not claimed as resolved local/member references; omitted syntax remains explicit
  follow-up work rather than low-confidence output.

## Deferred scope

This slice does not claim analyzer-equivalent lexical semantics. Receiver formals;
unparenthesized, pattern, or function-type closure parameter forms; pattern and multi-declarator loops;
single-statement and collection control-flow elements;
retroactive pre-declaration shadowing across earlier statements; definite-assignment and flow analysis;
member/index writes; destructuring, inherited members, extension lookup, implicit constructor selection,
nested generic internals, SDK/external namespaces, record and function-type internals, metadata
annotations, type inference, and overload resolution remain explicit follow-up work.
