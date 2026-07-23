use dartscope::{
    DartFileAnalysis, DartGraphqlContractAnalysis, DartLintAnalysis, DartProjectAnalysis,
    DartUriGraph, FlutterInventory, PubspecAnalysis, PubspecConfigurationAnalysis,
    PubspecDependencySource, PubspecFlutterConfiguration,
};

pub(super) const MAX_RETAINED_RESULT_ITEMS: usize = 2_000_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct ResultLimitExceeded {
    pub(super) context: &'static str,
    pub(super) max_items: usize,
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

    pub(super) fn check_graphql_contracts(
        &mut self,
        analysis: &DartGraphqlContractAnalysis,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("GraphQL contract bindings", analysis.bindings.len())?;
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
        self.charge("unresolved GraphQL uses", analysis.unresolved_uses.len())?;
        for unresolved in &analysis.unresolved_uses {
            self.charge(
                "unresolved GraphQL candidate paths",
                unresolved.candidate_paths.len(),
            )?;
        }
        Ok(())
    }

    pub(super) fn check_uri_graph(
        &mut self,
        graph: &DartUriGraph,
    ) -> Result<(), ResultLimitExceeded> {
        self.charge("URI graph references", graph.references.len())?;
        for reference in &graph.references {
            self.charge("URI graph candidate paths", reference.candidate_paths.len())?;
        }
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

    #[test]
    fn shares_one_budget_across_intermediate_and_final_results() {
        let project = DartProjectAnalysis {
            root: ".".to_string(),
            files: vec![DartFileAnalysis::empty("lib/main.dart")],
            pubspecs: Vec::new(),
            package_configs: Vec::new(),
            summary: Default::default(),
            diagnostics: Vec::new(),
        };
        let mut budget = AnalysisResultBudget::with_limit(1);

        budget.check_project_analysis(&project).unwrap();
        budget.check_uri_graph(&DartUriGraph::default()).unwrap();
        let error = budget.charge("later stage", 1).unwrap_err();

        assert_eq!(error.context, "later stage");
        assert_eq!(error.max_items, 1);
    }
}
