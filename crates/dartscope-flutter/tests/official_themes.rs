use dartscope_core::{DartFileInput, DartProjectInput};
use dartscope_flutter::{
    FlutterThemeApplicationKind, FlutterThemeConstructor, derive_flutter_theme_facts,
    extract_flutter_theme_facts,
};
use dartscope_parse::{analyze_file, analyze_project};

#[test]
fn derives_material_theme_constructions_and_application_slots() {
    let source = r#"import 'package:flutter/material.dart' as material;

final baseTheme = material.ThemeData(
  brightness: material.Brightness.light,
  colorSchemeSeed: const material.Color(0xff6750a4),
  useMaterial3: true,
);
final darkTheme = material.ThemeData.dark(useMaterial3: true);
final generatedScheme = material.ColorScheme.fromSeed(
  seedColor: const material.Color(0xff6750a4),
);
final highContrastScheme = material.ColorScheme.highContrastLight();
final generatedTheme = material.ThemeData.from(colorScheme: generatedScheme);

material.Widget buildApp() => material.MaterialApp(
  theme: baseTheme,
  darkTheme: darkTheme,
  highContrastTheme: material.ThemeData(colorScheme: highContrastScheme),
  highContrastDarkTheme: material.ThemeData.dark(),
  themeMode: material.ThemeMode.system,
);
"#;
    let file = analyze_file(DartFileInput::new("lib/theme.dart", source));
    let facts = derive_flutter_theme_facts(&file);

    assert_eq!(facts.constructions.len(), 5);
    assert_eq!(
        facts
            .constructions
            .iter()
            .map(|fact| fact.constructor)
            .collect::<Vec<_>>(),
        [
            FlutterThemeConstructor::ThemeData,
            FlutterThemeConstructor::ThemeDataDark,
            FlutterThemeConstructor::ThemeDataFrom,
            FlutterThemeConstructor::ThemeData,
            FlutterThemeConstructor::ThemeDataDark,
        ]
    );
    assert_eq!(
        facts.constructions[0].brightness.as_deref(),
        Some("material.Brightness.light")
    );
    assert_eq!(
        facts.constructions[0].color_scheme_seed.as_deref(),
        Some("const material.Color(0xff6750a4)")
    );
    assert_eq!(
        facts.constructions[0].use_material3.as_deref(),
        Some("true")
    );
    assert_eq!(
        facts.constructions[2].color_scheme.as_deref(),
        Some("generatedScheme")
    );

    assert_eq!(
        facts
            .applications
            .iter()
            .map(|fact| fact.application)
            .collect::<Vec<_>>(),
        [
            FlutterThemeApplicationKind::MaterialAppTheme,
            FlutterThemeApplicationKind::MaterialAppDarkTheme,
            FlutterThemeApplicationKind::MaterialAppHighContrastTheme,
            FlutterThemeApplicationKind::MaterialAppHighContrastDarkTheme,
            FlutterThemeApplicationKind::MaterialAppThemeMode,
        ]
    );
    assert_eq!(facts.applications[0].expression, "baseTheme");
    assert_eq!(
        facts.applications[4].expression,
        "material.ThemeMode.system"
    );
    assert!(
        facts
            .constructions
            .iter()
            .all(|fact| fact.span.byte_end > fact.span.byte_start)
    );
    assert!(
        facts
            .applications
            .iter()
            .all(|fact| fact.span.byte_end > fact.span.byte_start)
    );
}

#[test]
fn derives_subtree_theme_applications_without_evaluating_expressions() {
    let source = r#"import 'package:flutter/material.dart';

Widget themed(Widget child) => Theme(
  data: appTheme,
  child: AnimatedTheme(
    data: ThemeData.light(),
    child: child,
  ),
);
"#;
    let file = analyze_file(DartFileInput::new("lib/subtree.dart", source));
    let facts = derive_flutter_theme_facts(&file);

    assert_eq!(facts.constructions.len(), 1);
    assert_eq!(
        facts.constructions[0].constructor,
        FlutterThemeConstructor::ThemeDataLight
    );
    assert_eq!(facts.applications.len(), 2);
    assert_eq!(
        facts.applications[0].application,
        FlutterThemeApplicationKind::ThemeData
    );
    assert_eq!(facts.applications[0].expression, "appTheme");
    assert_eq!(
        facts.applications[1].application,
        FlutterThemeApplicationKind::AnimatedThemeData
    );
    assert_eq!(facts.applications[1].expression, "ThemeData.light()");
}

#[test]
fn theme_facts_require_the_official_material_import() {
    let source = r#"import 'package:flutter/widgets.dart';

void build() {
  ThemeData(useMaterial3: true);
  MaterialApp(theme: fakeTheme);
  Theme(data: fakeTheme, child: child);
}
"#;
    let file = analyze_file(DartFileInput::new("lib/plain.dart", source));

    assert_eq!(derive_flutter_theme_facts(&file), Default::default());
}

#[test]
fn project_theme_facts_are_sorted_by_path_and_source_position() {
    let first = "import 'package:flutter/material.dart';\nfinal theme = ThemeData.dark();\n";
    let second = "import 'package:flutter/material.dart';\nfinal theme = ThemeData.light();\n";
    let project = analyze_project(DartProjectInput::new(
        "fixture",
        vec![
            DartFileInput::new("lib/z.dart", first),
            DartFileInput::new("lib/a.dart", second),
        ],
        Vec::new(),
    ));
    let facts = extract_flutter_theme_facts(&project);

    assert_eq!(
        facts
            .constructions
            .iter()
            .map(|fact| fact.file_path.as_str())
            .collect::<Vec<_>>(),
        ["lib/a.dart", "lib/z.dart"]
    );
}
