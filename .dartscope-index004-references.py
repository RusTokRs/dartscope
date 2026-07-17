from pathlib import Path
import base64
import gzip
import hashlib

chunks = sorted(Path('.dartscope-index004-references').glob('chunk-*.txt'))
if len(chunks) != 16:
    raise SystemExit(f'expected 16 payload chunks, found {len(chunks)}')

encoded = ''.join(chunk.read_text(encoding='utf-8').strip() for chunk in chunks)
if len(encoded) != 9064:
    raise SystemExit(f'unexpected encoded payload length: {len(encoded)}')
if hashlib.sha256(encoded.encode('ascii')).hexdigest() != 'e5bd2af4942bfcd6160a63694a28440ec8ab6622648ca89091754eec7b1e798d':
    raise SystemExit('encoded payload checksum mismatch')

script = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(script).hexdigest() != '7375e0a0b1e4eb7a2f6836292600fb2c7c6691d0a038361d69380b433c349fe8':
    raise SystemExit('decoded patch checksum mismatch')

text = script.decode('utf-8')
replacements = [
    (
        """replace_once('crates/dartscope/src/lib.rs', '\\nPubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_project,\\nanalyze_project_with_parser, parse_normalized_dependency_source, parse_pubspec,\\n', '\\nPubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_file_with_references,\\nanalyze_project, analyze_project_with_parser, analyze_project_with_references,\\nparse_normalized_dependency_source, parse_pubspec,\\n')""",
        """replace_once('crates/dartscope/src/lib.rs', '\\n    PubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_project,\\n    analyze_project_with_parser, parse_normalized_dependency_source, parse_pubspec,\\n', '\\n    PubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_file_with_references,\\n    analyze_project, analyze_project_with_parser, analyze_project_with_references,\\n    parse_normalized_dependency_source, parse_pubspec,\\n')""",
    ),
    (
        """replace_once('crates/dartscope/src/lib.rs', '\\nDartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,\\nanalyze_part_links, build_uri_graph, build_uri_graph_with_options,\\n', '\\nDartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,\\nanalyze_part_links, build_uri_graph, build_uri_graph_with_options,\\nresolve_identifier_references, resolve_identifier_references_with_options,\\nresolve_project_identifier_references, resolve_project_identifier_references_with_options,\\nresolve_symbol, resolve_symbol_with_options,\\n')""",
        """replace_once('crates/dartscope/src/lib.rs', '\\n    DartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,\\n    analyze_part_links, build_uri_graph, build_uri_graph_with_options,\\n', '\\n    DartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,\\n    analyze_part_links, build_uri_graph, build_uri_graph_with_options,\\n    resolve_identifier_references, resolve_identifier_references_with_options,\\n    resolve_project_identifier_references, resolve_project_identifier_references_with_options,\\n    resolve_symbol, resolve_symbol_with_options,\\n')""",
    ),
    (
        """replace_once('README.md', '\\n- `dartscope-index` performs project-level linking over normalized analysis results. It resolves\\n  top-level Dart declarations through library namespaces and links GraphQL operation uses\\n  conservatively while comparing operation, client-call, and variable contracts without depending\\n  on parser internals.\\n', '\\n- `dartscope-index` performs project-level linking over normalized analysis results. It resolves\\n  top-level Dart declarations through library namespaces, batch-resolves opt-in conservative\\n  invocation-target references, and links GraphQL operation uses while comparing operation,\\n  client-call, and variable contracts without depending on parser internals.\\n')""",
        """replace_once('README.md', '\\n- `dartscope-index` performs project-level linking over normalized analysis results. A shared\\n  namespace engine resolves top-level Dart declarations and GraphQL operation constants through\\n  same-library, import, re-export, combinator, privacy, part, and conditional-environment semantics;\\n  GraphQL linking also compares operation, client-call, and variable contracts without parser internals.\\n', '\\n- `dartscope-index` performs project-level linking over normalized analysis results. A shared\\n  namespace engine resolves top-level Dart declarations, batch-resolves opt-in conservative\\n  invocation-target references, and links GraphQL operation constants through same-library, import,\\n  re-export, combinator, privacy, part, and conditional-environment semantics; GraphQL linking also\\n  compares operation, client-call, and variable contracts without parser internals.\\n')""",
    ),
]
for old, new in replacements:
    if text.count(old) != 1:
        raise SystemExit('expected one anchor repair')
    text = text.replace(old, new)

fixed = text.encode('utf-8')
if hashlib.sha256(fixed).hexdigest() != 'cc6deac3f6ca725e9741e15bcae311224239b8df9705c85dec61dc79d389163d':
    raise SystemExit('fixed patch checksum mismatch')

exec(compile(fixed, '.dartscope-index004-references-payload.py', 'exec'))
