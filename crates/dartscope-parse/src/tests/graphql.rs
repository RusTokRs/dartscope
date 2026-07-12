use crate::analyze_file;
use dartscope_core::*;

#[test]
fn extracts_graphql_operations_from_raw_string_constants() {
    let source = r#"
const storefrontMobileCatalogQuery = r'''
  query StorefrontMobileCatalog($input: SearchPreviewInput!) {
    storefrontSearch(input: $input) {
      items {
        id
        title
      }
    }
  }
''';

const storefrontMobileCreateCartMutation = r'''
  mutation StorefrontMobileCreateCart($input: CreateStorefrontCartInput!) {
    createStorefrontCart(input: $input) {
      cart {
        id
      }
    }
  }
''';
"#;

    let analysis = analyze_file(DartFileInput::new(
        "lib/data/storefront_catalog_repository.dart",
        source,
    ));

    assert_eq!(analysis.graphql_operations.len(), 2);
    assert!(analysis.graphql_operations.iter().any(|operation| {
        operation.constant_name == "storefrontMobileCatalogQuery"
            && operation.operation_type == DartGraphqlOperationType::Query
            && operation.operation_name.as_deref() == Some("StorefrontMobileCatalog")
            && operation.variable_names == ["input"]
            && operation.root_fields == ["storefrontSearch"]
    }));
    assert!(analysis.graphql_operations.iter().any(|operation| {
        operation.constant_name == "storefrontMobileCreateCartMutation"
            && operation.operation_type == DartGraphqlOperationType::Mutation
            && operation.operation_name.as_deref() == Some("StorefrontMobileCreateCart")
            && operation.variable_names == ["input"]
            && operation.root_fields == ["createStorefrontCart"]
    }));
}

const REPOSITORY_METHODS_SOURCE: &str = r#"
const moduleRegistryQuery = r'''
  query ModuleRegistry {
    moduleRegistry {
      moduleSlug
    }
  }
''';

const toggleModuleMutation = r'''
  mutation ToggleModule($moduleSlug: String!, $enabled: Boolean!) {
    toggleModule(moduleSlug: $moduleSlug, enabled: $enabled) {
      moduleSlug
    }
  }
''';

class GraphQlModulesRepository {
  Future<List<Object>> listModules() async {
    final result = await _client.query(
      QueryOptions(
        document: gql(moduleRegistryQuery),
      ),
    );
    return const <Object>[];
  }

  Future<Object> toggleModule() async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(toggleModuleMutation),
        variables: <String, dynamic>{
          'moduleSlug': moduleSlug,
          'enabled': enabled,
        },
      ),
    );
    return Object();
  }

  Future<Object> compensateModule(
    String moduleSlug,
  ) async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(compensateModuleMutation),
        variables: <String, dynamic>{'operationId': operationId},
      ),
    );
    return Object();
  }

  Future<Object> createCart() async {
    final result = await _client.mutate(
      MutationOptions(
        document: gql(createCartMutation),
        variables: <String, dynamic>{
          'input': <String, dynamic>{
            'email': email,
            'locale': locale,
          },
        },
      ),
    );
    return Object();
  }
}

final inlineOptions = MutationOptions(
  document: gql(r'''
    mutation InlineRefresh {
      refreshToken {
        accessToken
      }
    }
  '''),
);
"#;

#[test]
fn links_graphql_operation_constants_to_repository_methods() {
    let analysis = analyze_file(DartFileInput::new(
        "lib/src/modules_repository.dart",
        REPOSITORY_METHODS_SOURCE,
    ));

    assert_eq!(analysis.graphql_operation_uses.len(), 4);
    assert!(analysis.graphql_operation_uses.iter().any(|usage| {
        usage.constant_name == "moduleRegistryQuery"
            && usage.client_call == DartGraphqlClientCall::Query
            && usage.variable_names.is_empty()
            && usage.enclosing_callable.as_deref() == Some("listModules")
            && usage.enclosing_symbol.as_ref().is_some_and(|symbol| {
                symbol.name == "listModules" && symbol.kind == DartEnclosingSymbolKind::Callable
            })
    }));
    assert!(analysis.graphql_operation_uses.iter().any(|usage| {
        usage.constant_name == "toggleModuleMutation"
            && usage.client_call == DartGraphqlClientCall::Mutation
            && usage.variable_names == ["enabled", "moduleSlug"]
            && usage.enclosing_callable.as_deref() == Some("toggleModule")
    }));
    assert!(analysis.graphql_operation_uses.iter().any(|usage| {
        usage.constant_name == "compensateModuleMutation"
            && usage.client_call == DartGraphqlClientCall::Mutation
            && usage.variable_names == ["operationId"]
            && usage.enclosing_callable.as_deref() == Some("compensateModule")
    }));
    assert!(analysis.graphql_operation_uses.iter().any(|usage| {
        usage.constant_name == "createCartMutation"
            && usage.client_call == DartGraphqlClientCall::Mutation
            && usage.variable_names == ["input"]
            && usage.enclosing_callable.as_deref() == Some("createCart")
    }));
}

#[test]
fn links_graphql_operation_use_to_top_level_provider_initializer() {
    let source = r#"
const bootstrapProbeDocument = r'''
  query BootstrapProbe {
    me {
      id
    }
  }
''';

final authBootstrapProbeProvider = FutureProvider<BootstrapProbeResult>((
  ref,
) async {
  final result = await client.query(
    QueryOptions(
      document: gql(bootstrapProbeDocument),
    ),
  );
  return BootstrapProbeResult();
});
"#;

    let analysis = analyze_file(DartFileInput::new(
        "lib/app_shell/auth_bootstrap.dart",
        source,
    ));

    assert_eq!(analysis.graphql_operation_uses.len(), 1);
    let usage = &analysis.graphql_operation_uses[0];
    assert_eq!(usage.constant_name, "bootstrapProbeDocument");
    assert_eq!(usage.client_call, DartGraphqlClientCall::Query);
    assert!(usage.variable_names.is_empty());
    assert_eq!(usage.enclosing_callable, None);
    assert!(usage.enclosing_symbol.as_ref().is_some_and(|symbol| {
        symbol.name == "authBootstrapProbeProvider"
            && symbol.kind == DartEnclosingSymbolKind::Variable
    }));
}
