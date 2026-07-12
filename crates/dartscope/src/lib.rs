pub use dartscope_core::*;

#[cfg(feature = "parse")]
pub use dartscope_parse::{
    analyze_file, analyze_project, analyze_project_with_parser, parse_pubspec,
    DartLanguageVersionCoverage, DartParser, DartParserCapability, DartParserCapabilityStatus,
    DartParserCapabilitySupport, DartParserMetadata, HeuristicDartParser,
};

#[cfg(feature = "resolve")]
pub use dartscope_resolve::{parse_package_config, resolve_package_uri, PackageUriResolutionError};

#[cfg(feature = "index")]
pub use dartscope_index::{
    analyze_graphql_contracts, analyze_graphql_contracts_with_options, analyze_part_links,
    build_uri_graph, build_uri_graph_with_options, DartIndexOptions,
};

#[cfg(feature = "json")]
pub use dartscope_json::{to_json, to_json_pretty};

#[cfg(feature = "flutter")]
pub use dartscope_flutter::{
    extract_flutter_inventory, FlutterAssetEntry, FlutterInventory, FlutterInventorySummary,
    FlutterLocalizationEntry, FlutterRouteEntry, FlutterWidgetEntry,
};
