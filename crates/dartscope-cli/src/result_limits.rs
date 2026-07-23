use dartscope::{
    DartFileAnalysis, DartGraphqlContractAnalysis, DartIndexOptions, DartLintAnalysis,
    DartProjectAnalysis, DartUriGraph, FlutterInventory, PubspecAnalysis,
    PubspecConfigurationAnalysis, PubspecDependencySource, PubspecFlutterConfiguration,
};

pub(super) const MAX_RETAINED_RESULT_ITEMS: usize = 2_000_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct ResultLimitExceeded {
    pub(super) context: &'static str,
    pub(super) max_items: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct GraphqlContractReservation {
    results: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct UriGraphReservation {
    references: usize,
}

#[derive(Debug)]
pub(super) struct AnalysisResultBudget {
    items: usize,
    max_items: usize,
}

impl Default for AnalysisResultBudget {
    fn default() -> Self {
        Self {
            items: 0,
            max_items: MAX_RETAINED_RESULT_ITEMS,
        }
    }
}

impl AnalysisResultBudget {
    #[cfg(test)]
    const fn with_limit(max_items: usize) -> Self {
        Self {
            items: 0,
            max_items,
        }
    }

    fn charge(&mut self, context: &'static str, count: usize) -> Result<(), ResultLimitExceeded> {
        let Some(next) = self.items.checked_add(count) else {
            return Err(self.limit_error(context));
        };
        if next > self.max_items {
            return Err(self.limit_error(context));
        }
        self.items = next;
        Ok(())
    }

    fn limit_error(&self, context: &'static str) -> ResultLimitExceeded {
        ResultLimitExceeded {
            context,
            max_items: self.max_items,
        }
    }

    pub(super) fn check_file_analysis(
        &mut self,
        analysis: &DartFileAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("file imports", analysis.imports.len())?;
        for import in &analysis.imports {
            self.charge("import configurations", import.configurations.len())?;
            self.charge("import combinators", import.combinators.len())?;
            for combinator in &import.combinators {
                self.charge("import combinator names", combinator.names.len())?;
            }
        }

        self.charge("file exports", analysis.exports.len())?;
        for export in &analysis.exports {
            self.charge("export configurations", export.configurations.len())?;
            self.charge("export combinators", export.combinators.len())?;
            for combinator in &export.combinators {
                self.charge("export combinator names", combinator.names.len())?;
            }
        }

        self.charge("file parts", analysis.parts.len())?;
        self.charge("file declarations", analysis.declarations.len())?;
        for declaration in &analysis.declarations {
            self.charge("declaration mixins", declaration.mixes_in.len())?;
        }
        self.charge("file string constants", analysis.string_constants.len())?;

        self.charge("file GraphQL operations", analysis.graphql_operations.len())?;
        for operation in &analysis.graphql_operations {
            self.charge(
                "GraphQL operation variables",
                operation.variable_names.len(),
            )?;
            self.charge("GraphQL operation root fields", operation.root_fields.len())?;
        }

        self.charge(
            "file GraphQL operation uses",
            analysis.graphql_operation_uses.len(),
        )?;
        for operation_use in &analysis.graphql_operation_uses {
            self.charge("GraphQL use variables", operation_use.variable_names.len())?;
        }

        self.charge("file invocations", analysis.invocations.len())?;
        for invocation in &analysis.invocations {
            self.charge("invocation arguments", invocation.arguments.len())?;
            self.charge("invocation result members", invocation.result_members.len())?;
            for argument in &invocation.arguments {
                self.charge("invocation map entries", argument.map_entries.len())?;
            }
        }

        self.charge("file Flutter widgets", analysis.flutter.widgets.len())?;
        self.charge("file Flutter routes", analysis.flutter.routes.len())?;
        self.charge("file Flutter assets", analysis.flutter.assets.len())?;
        self.charge(
            "file Flutter localizations",
            analysis.flutter.localizations.len(),
        )?;
        self.charge("file diagnostics", analysis.diagnostics.len())
    }

    pub(super) fn check_pubspec_analysis(
        &mut self,
        analysis: &PubspecAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("pubspec dependencies", analysis.dependencies.len())?;
        for dependency in &analysis.dependencies {
            match dependency.source.as_ref() {
                Some(PubspecDependencySource::Git {
                    additional_fields, ..
                })
                | Some(PubspecDependencySource::Hosted {
                    additional_fields, ..
                }) => {
                    self.charge("pubspec dependency source fields", additional_fields.len())?;
                }
                _ => {}
            }
        }
        self.check_pubspec_flutter_configuration(&analysis.configuration.flutter)?;
        self.charge(
            "pubspec environment constraints",
            analysis.configuration.environment.len(),
        )?;
        self.charge("pubspec diagnostics", analysis.diagnostics.len())
    }

    pub(super) fn check_pubspec_configuration(
        &mut self,
        analysis: &PubspecConfigurationAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge(
            "pubspec environment constraints",
            analysis.environment.len(),
        )?;
        self.check_pubspec_flutter_configuration(&analysis.flutter)?;
        self.charge("pubspec diagnostics", analysis.diagnostics.len())
    }

    fn check_pubspec_flutter_configuration(
        &mut self,
        flutter: &PubspecFlutterConfiguration,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("pubspec Flutter assets", flutter.assets.len())?;
        self.charge(
            "pubspec Flutter asset configurations",
            flutter.asset_configurations.len(),
        )?;
        for asset in &flutter.asset_configurations {
            self.charge("Flutter asset flavors", asset.flavors.len())?;
            self.charge("Flutter asset platforms", asset.platforms.len())?;
            self.charge("Flutter asset transformers", asset.transformers.len())?;
            for transformer in &asset.transformers {
                self.charge(
                    "Flutter asset transformer arguments",
                    transformer.args.len(),
                )?;
            }
        }
        self.charge("pubspec Flutter font families", flutter.fonts.len())?;
        for family in &flutter.fonts {
            self.charge("pubspec Flutter font files", family.fonts.len())?;
        }
        Ok(())
    }

    pub(super) fn check_project_analysis(
        &mut self,
        analysis: &DartProjectAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("project files", analysis.files.len())?;
        for file in &analysis.files {
            self.check_file_analysis(file)?;
        }

        self.charge("project pubspecs", analysis.pubspecs.len())?;
        for pubspec in &analysis.pubspecs {
            self.check_pubspec_analysis(pubspec)?;
        }

        self.charge(
            "project package configurations",
            analysis.package_configs.len(),
        )?;
        for package_config in &analysis.package_configs {
            self.charge(
                "package configuration entries",
                package_config.packages.len(),
            )?;
            self.charge(
                "package configuration diagnostics",
                package_config.diagnostics.len(),
            )?;
        }
        self.charge("project diagnostics", analysis.diagnostics.len())
    }

    pub(super) fn preflight_graphql_contracts(
        &mut self,
        project: &DartProjectAnalysis,
    ) -> Result<GraphqlContractReservation, ResultLimitExceeded> {
        let mut results = 0usize;
        for file in &project.files {
            self.add_projection(
                &mut results,
                file.graphql_operation_uses.len(),
                "GraphQL contract results",
            )?;
        }
        self.charge("GraphQL contract results", results)?;
        Ok(GraphqlContractReservation { results })
    }

    pub(super) fn check_graphql_contracts(
        &mut self,
        analysis: &DartGraphqlContractAnalysis,
        reservation: GraphqlContractReservation,
    ) -> Result<(), ResultLimitExceeded> {
        let Some(results) = analysis
            .bindings
            .len()
            .checked_add(analysis.unresolved_uses.len())
        else {
            return Err(self.limit_error("GraphQL contract results"));
        };
        if results > reservation.results {
            self.charge("GraphQL contract results", results - reservation.results)?;
        }
        debug_assert_eq!(results, reservation.results);

        for binding in &analysis.bindings {
            self.charge(
                "GraphQL declared variables",
                binding.declared_variable_names.len(),
            )?;
            self.charge(
                "GraphQL supplied variables",
                binding.supplied_variable_names.len(),
            )?;
            self.charge(
                "GraphQL missing variables",
                binding.missing_variable_names.len(),
            )?;
            self.charge(
                "GraphQL unexpected variables",
                binding.unexpected_variable_names.len(),
            )?;
        }
        for unresolved in &analysis.unresolved_uses {
            self.charge(
                "unresolved GraphQL candidate paths",
                unresolved.candidate_paths.len(),
            )?;
        }
        Ok(())
    }

    pub(super) fn preflight_uri_graph(
        &mut self,
        project: &DartProjectAnalysis,
        options: &DartIndexOptions,
    ) -> Result<UriGraphReservation, ResultLimitExceeded> {
        let mut references = 0usize;
        let environment_selected = options.compilation_environment.is_some();

        for file in &project.files {
            if environment_selected {
                self.add_projection(&mut references, file.imports.len(), "URI graph references")?;
                self.add_projection(&mut references, file.exports.len(), "URI graph references")?;
            } else {
                for import in &file.imports {
                    self.add_projection(&mut references, 1, "URI graph references")?;
                    self.add_projection(
                        &mut references,
                        import.configurations.len(),
                        "URI graph references",
                    )?;
                }
                for export in &file.exports {
                    self.add_projection(&mut references, 1, "URI graph references")?;
                    self.add_projection(
                        &mut references,
                        export.configurations.len(),
                        "URI graph references",
                    )?;
                }
            }
            self.add_projection(&mut references, file.parts.len(), "URI graph references")?;
        }

        self.charge("URI graph references", references)?;
        Ok(UriGraphReservation { references })
    }

    pub(super) fn check_uri_graph(
        &mut self,
        graph: &DartUriGraph,
        reservation: UriGraphReservation,
    ) -> Result<(), ResultLimitExceeded> {
        if graph.references.len() > reservation.references {
            self.charge(
                "URI graph references",
                graph.references.len() - reservation.references,
            )?;
        }
        debug_assert_eq!(graph.references.len(), reservation.references);
        for reference in &graph.references {
            self.charge("URI graph candidate paths", reference.candidate_paths.len())?;
        }
        Ok(())
    }

    fn add_projection(
        &self,
        total: &mut usize,
        count: usize,
        context: &'static str,
    ) -> Result<(), ResultLimitExceeded> {
        let Some(next) = total.checked_add(count) else {
            return Err(self.limit_error(context));
        };
        *total = next;
        Ok(())
    }

    pub(super) fn check_flutter_inventory(
        &mut self,
        inventory: &FlutterInventory,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("Flutter inventory widgets", inventory.widgets.len())?;
        self.charge("Flutter inventory routes", inventory.routes.len())?;
        self.charge("Flutter inventory assets", inventory.assets.len())?;
        for asset in &inventory.assets {
            self.charge(
                "linked Flutter asset declarations",
                usize::from(asset.declaration.is_some()),
            )?;
            if let Some(declaration) = &asset.declaration {
                self.charge("linked Flutter asset flavors", declaration.flavors.len())?;
                self.charge(
                    "linked Flutter asset platforms",
                    declaration.platforms.len(),
                )?;
            }
        }

        self.charge(
            "Flutter inventory localizations",
            inventory.localizations.len(),
        )?;
        for localization in &inventory.localizations {
            self.charge(
                "Flutter localization catalog paths",
                localization.catalog_paths.len(),
            )?;
        }

        self.charge(
            "Flutter-importing file paths",
            inventory.flutter_file_paths.len(),
        )?;
        self.charge(
            "Flutter asset declarations",
            inventory.asset_declarations.len(),
        )?;
        for declaration in &inventory.asset_declarations {
            self.charge(
                "Flutter asset declaration flavors",
                declaration.flavors.len(),
            )?;
            self.charge(
                "Flutter asset declaration platforms",
                declaration.platforms.len(),
            )?;
        }

        self.charge(
            "Flutter localization configurations",
            inventory.l10n_configurations.len(),
        )?;
        self.charge("Flutter ARB catalogs", inventory.arb_catalogs.len())?;
        for catalog in &inventory.arb_catalogs {
            self.charge("Flutter ARB messages", catalog.messages.len())?;
        }
        self.charge("Flutter inventory diagnostics", inventory.diagnostics.len())
    }

    pub(super) fn check_lint_analysis(
        &mut self,
        analysis: &DartLintAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("lint diagnostics", analysis.diagnostics.len())?;
        for diagnostic in &analysis.diagnostics {
            self.charge("lint related paths", diagnostic.related_paths.len())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_exact_item_boundary_and_rejects_the_next_item() {
        let mut budget = AnalysisResultBudget::with_limit(3);

        budget.charge("first collection", 2).unwrap();
        budget.charge("exact boundary", 1).unwrap();
        let error = budget.charge("next collection", 1).unwrap_err();

        assert_eq!(
            error,
            ResultLimitExceeded {
                context: "next collection",
                max_items: 3,
            }
        );
    }

    fn graphql_contract_project() -> DartProjectAnalysis {
        use dartscope::{DartFileInput, DartProjectInput, analyze_project};

        analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/api.dart",
                r#"
const viewerQuery = r'''query Viewer { viewer { id } }''';
void load() {
  client.query(QueryOptions(document: gql(viewerQuery)));
  client.query(QueryOptions(document: gql(missingQuery)));
}
"#,
            )],
            Vec::new(),
        ))
    }

    #[test]
    fn graphql_contract_preflight_rejects_before_building_result_vectors() {
        let project = graphql_contract_project();
        let mut budget = AnalysisResultBudget::with_limit(1);

        let error = budget.preflight_graphql_contracts(&project).unwrap_err();

        assert_eq!(error.context, "GraphQL contract results");
        assert_eq!(error.max_items, 1);
    }

    #[test]
    fn graphql_contract_preflight_matches_output_without_double_charge() {
        use dartscope::analyze_graphql_contracts;

        let project = graphql_contract_project();
        let mut budget = AnalysisResultBudget::with_limit(2);

        let reservation = budget.preflight_graphql_contracts(&project).unwrap();
        assert_eq!(reservation.results, 2);
        let analysis = analyze_graphql_contracts(&project);
        assert_eq!(analysis.bindings.len(), 1);
        assert_eq!(analysis.unresolved_uses.len(), 1);
        budget
            .check_graphql_contracts(&analysis, reservation)
            .unwrap();
        let error = budget.charge("later stage", 1).unwrap_err();

        assert_eq!(error.context, "later stage");
        assert_eq!(error.max_items, 2);
    }

    fn conditional_uri_project() -> DartProjectAnalysis {
        use dartscope::{DartFileInput, DartProjectInput, analyze_project};

        analyze_project(DartProjectInput::new(
            ".",
            vec![DartFileInput::new(
                "lib/main.dart",
                concat!(
                    "import 'dart:async' if (dart.library.io) 'dart:io' ",
                    "if (dart.library.html) 'dart:html';\n",
                    "export 'dart:core' if (dart.library.io) 'dart:collection';\n",
                ),
            )],
            Vec::new(),
        ))
    }

    #[test]
    fn uri_graph_preflight_rejects_before_building_the_reference_vector() {
        let project = conditional_uri_project();
        let options = DartIndexOptions::default();
        let mut budget = AnalysisResultBudget::with_limit(4);

        let error = budget.preflight_uri_graph(&project, &options).unwrap_err();

        assert_eq!(error.context, "URI graph references");
        assert_eq!(error.max_items, 4);
    }

    #[test]
    fn uri_graph_preflight_matches_environment_selected_output_without_double_charge() {
        use dartscope::{DartCompilationEnvironment, build_uri_graph_with_options};

        let project = conditional_uri_project();
        let options = DartIndexOptions::default().with_compilation_environment(
            DartCompilationEnvironment::from_pairs([("dart.library.io", "true")]),
        );
        let mut budget = AnalysisResultBudget::with_limit(2);

        let reservation = budget.preflight_uri_graph(&project, &options).unwrap();
        assert_eq!(reservation.references, 2);
        let graph = build_uri_graph_with_options(&project, &options);
        assert_eq!(graph.references.len(), 2);
        budget.check_uri_graph(&graph, reservation).unwrap();
        let error = budget.charge("later stage", 1).unwrap_err();

        assert_eq!(error.context, "later stage");
        assert_eq!(error.max_items, 2);
    }
}
