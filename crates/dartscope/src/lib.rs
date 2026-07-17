pub use dartscope_core::pubspec::PubspecConfiguration;
pub use dartscope_core::*;

#[cfg(feature = "parse")]
pub use dartscope_parse::{
    DartLanguageVersionCoverage, DartParser, DartParserCapability, DartParserCapabilityStatus,
    DartParserCapabilitySupport, DartParserMetadata, HeuristicDartParser,
    PubspecConfigurationAnalysis, PubspecDependencySource, PubspecDependencySourceExt,
    PubspecDependencySourceField, PubspecEnvironmentConstraint, PubspecFlutterAsset,
    PubspecFlutterAssetConfiguration, PubspecFlutterAssetTransformer, PubspecFlutterConfiguration,
    PubspecFlutterFont, PubspecFlutterFontFamily, analyze_file, analyze_file_with_references,
    analyze_project, analyze_project_with_parser, analyze_project_with_references,
    parse_normalized_dependency_source, parse_pubspec, parse_pubspec_configuration,
};

#[cfg(feature = "resolve")]
pub use dartscope_resolve::{PackageUriResolutionError, parse_package_config, resolve_package_uri};

#[cfg(feature = "index")]
pub use dartscope_index::{
    DartIndexOptions, analyze_graphql_contracts, analyze_graphql_contracts_with_options,
    analyze_part_links, build_uri_graph, build_uri_graph_with_options,
    resolve_identifier_references, resolve_identifier_references_with_options,
    resolve_project_identifier_references, resolve_project_identifier_references_with_options,
    resolve_symbol, resolve_symbol_with_options,
};

#[cfg(feature = "json")]
pub use dartscope_json::{
    JsonContract, VersionedJsonEnvelope, to_json, to_json_contract, to_json_contract_pretty,
    to_json_pretty,
};

#[cfg(feature = "flutter")]
pub use dartscope_flutter::{
    FlutterArbCatalog, FlutterArbInput, FlutterArbMessage, FlutterAssetDeclarationEntry,
    FlutterAssetDeclarationKind, FlutterAssetDeclarationRef, FlutterAssetEntry,
    FlutterCatalogInput, FlutterInventory, FlutterInventorySummary, FlutterL10nConfiguration,
    FlutterL10nInput, FlutterLocalizationEntry, FlutterRouteEntry, FlutterWidgetEntry,
    extract_flutter_inventory, extract_flutter_inventory_with_catalogs,
};

/// Parses one Dart file and explicitly applies optional Flutter convention extraction.
#[cfg(all(feature = "parse", feature = "flutter"))]
pub fn analyze_file_with_flutter(input: DartFileInput) -> DartFileAnalysis {
    let mut analysis = dartscope_parse::analyze_file(input);
    dartscope_flutter::populate_flutter_file_hints(&mut analysis);
    analysis
}

/// Parses a project and explicitly applies optional Flutter convention extraction.
#[cfg(all(feature = "parse", feature = "flutter"))]
pub fn analyze_project_with_flutter(input: DartProjectInput) -> DartProjectAnalysis {
    let mut analysis = dartscope_parse::analyze_project(input);
    dartscope_flutter::populate_flutter_project_analysis(&mut analysis);
    analysis
}

#[cfg(all(test, feature = "parse", feature = "flutter"))]
mod tests {
    use super::{
        DartFileInput, DartProjectInput, analyze_file, analyze_file_with_flutter,
        analyze_project_with_flutter,
    };

    #[test]
    fn explicit_flutter_composition_keeps_pure_parser_independent() {
        let source =
            "import 'package:flutter/widgets.dart';\nclass App extends StatelessWidget {}\n";
        let pure = analyze_file(DartFileInput::new("lib/app.dart", source));
        let composed = analyze_file_with_flutter(DartFileInput::new("lib/app.dart", source));

        assert!(pure.flutter.widgets.is_empty());
        assert_eq!(composed.flutter.widgets.len(), 1);

        let project = analyze_project_with_flutter(DartProjectInput::new(
            "demo",
            vec![DartFileInput::new("lib/app.dart", source)],
            Vec::new(),
        ));
        assert_eq!(project.summary.flutter_widgets, 1);
    }
}
