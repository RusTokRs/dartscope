# Parser complexity limits

The built-in heuristic Dart parser is intentionally source-only and non-recursive, but declaration
inventory has repeated line-oriented passes: one top-level pass, one member pass for each discovered
type, and one local-variable pass for each discovered callable. Pathological source can therefore
turn a bounded byte input into disproportionate declaration-scanning work.

## Declaration scan budget

Each file has an inclusive budget of **10,000,000 declaration line inspections**. One inspection is
charged whenever a source line is visited by:

- the top-level declaration scan;
- a type member scan;
- a callable local-variable scan.

The budget counts the actual repeated work rather than source lines alone. A file exactly at the
budget is accepted. The next inspection fails deterministically.

## Failure behavior

When the budget is exceeded, declaration scanning stops and returns no declaration inventory for
that file. The file receives an error diagnostic with the stable code
`analysis_complexity_limit_exceeded`. Other facts already produced by independent linear passes may
remain available, so consumers must treat that diagnostic as an incomplete analysis result.

The parser API remains filesystem-free and keeps its existing diagnostic-based recovery contract.
The CLI `lint` command treats parser error diagnostics as malformed project input; structured
analysis commands retain the diagnostic in their versioned JSON output.

This budget is separate from the CLI filesystem input and output limits. Applications calling the
library directly receive the same parser work bound even when they provide in-memory source larger
than the CLI permits.
