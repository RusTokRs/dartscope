pub use dartscope_core::pubspec::PubspecConfiguration;
pub use dartscope_core::*;

#[cfg(feature = "parse")]
pub use dartscope_parse::{
    DartLanguageVersionCoverage, DartParser, DartParserCapability, DartParserCapabilityStatus,
    DartParserCapabilitySupport, DartParserMetadata, HeuristicDartParser,
    PubspecConfigurationAnalysis, PubspecDependencySource, PubspecDependencySourceExt,
    PubspecDependencySourceField, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_project,
    analyze_project_with_parser, parse_normalized_dependency_source, parse_pubspec,
    parse_pubspec_configuration,
};

#[cfg(feature = "resolve")]
pub use dartscope_resolve::{PackageUriResolutionError, parse_package_config, resolve_package_uri};

#[cfg(feature = "index")]
pub use dartscope_index::{
    DartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,
    analyze_part_links, build_uri_graph, build_uri_graph_with_options,
};

#[cfg(feature = "json")]
pub use dartscope_json::{
    JsonContract, VersionedJsonEnvelope, to_json, to_json_contract, to_json_contract_pretty,
    to_json_pretty,
};

#[cfg(feature = "flutter")]
pub use dartscope_flutter::{
    FlutterAssetEntry, FlutterInventory, FlutterInventorySummary, FlutterLocalizationEntry,
    FlutterRouteEntry, FlutterWidgetEntry, extract_flutter_inventory,
};
