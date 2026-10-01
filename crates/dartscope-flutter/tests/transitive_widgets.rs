//! A class that reaches a Flutter base class through other classes of the project is a widget too
//! (audit 2026-09-30, section 6.2). The chain is followed by class name through the project; a
//! mixin cannot change the superclass, so `with` does not make a widget.

use dartscope_core::{Confidence, DartFileInput, DartProjectAnalysis, DartProjectInput};
use dartscope_flutter::{
    FlutterInventory, derive_flutter_file_hints, extract_flutter_inventory,
    populate_flutter_project_analysis,
};
use dartscope_parse::{analyze_file, analyze_project};

fn project(files: &[(&str, &str)]) -> DartProjectAnalysis {
    analyze_project(DartProjectInput::new(
        ".",
        files
            .iter()
            .map(|(path, source)| DartFileInput::new(*path, *source))
            .collect(),
        Vec::new(),
    ))
}

fn widgets(inventory: &FlutterInventory) -> Vec<String> {
    inventory
        .widgets
        .iter()
        .map(|widget| {
            format!(
                "{}:{}<-{}{}",
                widget.file_path,
                widget.class_name,
                widget.base_class,
                widget
                    .inherited_via
                    .as_deref()
                    .map(|via| format!(" via {via}"))
                    .unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn a_subclass_of_a_project_widget_is_a_widget_with_medium_confidence() {
    let project = project(&[(
        "lib/screens.dart",
        concat!(
            "import 'package:flutter/widgets.dart';\n",
            "abstract class BaseScreen extends StatelessWidget {}\n",
            "class Screen extends BaseScreen {}\n",
            "class Plain {}\n",
        ),
    )]);

    let inventory = extract_flutter_inventory(&project);

    assert_eq!(
        widgets(&inventory),
        [
            "lib/screens.dart:BaseScreen<-StatelessWidget",
            "lib/screens.dart:Screen<-StatelessWidget via BaseScreen",
        ]
    );
    assert_eq!(inventory.widgets[0].confidence, Confidence::High);
    assert_eq!(inventory.widgets[1].confidence, Confidence::Medium);
    assert_eq!(inventory.summary.widgets, 2);
}

#[test]
fn the_chain_crosses_files_and_any_number_of_levels() {
    let project = project(&[
        ("lib/a.dart", "class Third extends Second {}\n"),
        (
            "lib/b.dart",
            "import 'c.dart';\nclass Second extends First {}\n",
        ),
        (
            "lib/c.dart",
            "import 'package:flutter/widgets.dart';\nclass First extends StatefulWidget {}\n",
        ),
        ("lib/d.dart", "import 'a.dart';\nclass Fourth extends Third {}\n"),
    ]);

    let inventory = extract_flutter_inventory(&project);

    assert_eq!(
        widgets(&inventory),
        [
            "lib/a.dart:Third<-StatefulWidget via Second",
            "lib/b.dart:Second<-StatefulWidget via First",
            "lib/c.dart:First<-StatefulWidget",
            "lib/d.dart:Fourth<-StatefulWidget via Third",
        ]
    );
}

#[test]
fn populating_the_project_stores_the_same_hints_and_counts_them() {
    let mut project = project(&[
        (
            "lib/base.dart",
            "import 'package:flutter/widgets.dart';\nclass Base extends StatelessWidget {}\n",
        ),
        ("lib/screen.dart", "import 'base.dart';\nclass Screen extends Base {}\n"),
    ]);
    assert_eq!(project.summary.flutter_widgets, 0);

    populate_flutter_project_analysis(&mut project);

    assert_eq!(project.summary.flutter_widgets, 2);
    let screen = &project.files[1].flutter.widgets;
    assert_eq!(screen.len(), 1);
    assert_eq!(screen[0].class_name, "Screen");
    assert_eq!(screen[0].base_class, "StatelessWidget");
    assert_eq!(screen[0].inherited_via.as_deref(), Some("Base"));
}

#[test]
fn a_single_file_follows_the_classes_it_declares_itself() {
    let source = "class Base extends State {}\nclass Derived extends Base {}\nclass Elsewhere extends NotHere {}\n";
    let file = analyze_file(DartFileInput::new("lib/x.dart", source));

    let hints = derive_flutter_file_hints(&file);

    let names: Vec<_> = hints
        .widgets
        .iter()
        .map(|widget| widget.class_name.as_str())
        .collect();
    assert_eq!(names, ["Base", "Derived"]);
}

#[test]
fn only_a_unique_visible_class_is_followed() {
    let project = project(&[
        (
            "lib/private.dart",
            "import 'package:flutter/widgets.dart';\nclass _Base extends StatelessWidget {}\nclass Inside extends _Base {}\n",
        ),
        // `_Base` belongs to private.dart, so this one means some other library's class.
        ("lib/outside.dart", "class Outside extends _Base {}\n"),
        // Two public classes of one name cannot be told apart without the import graph.
        (
            "lib/one.dart",
            "import 'package:flutter/widgets.dart';\nclass Shared extends StatelessWidget {}\n",
        ),
        ("lib/two.dart", "class Shared {}\n"),
        ("lib/user.dart", "class User extends Shared {}\n"),
        // A superclass that is not part of the project cannot be judged.
        ("lib/unknown.dart", "class Unknown extends Missing {}\n"),
    ]);

    let inventory = extract_flutter_inventory(&project);

    assert_eq!(
        widgets(&inventory),
        [
            "lib/one.dart:Shared<-StatelessWidget",
            "lib/private.dart:_Base<-StatelessWidget",
            "lib/private.dart:Inside<-StatelessWidget via _Base",
        ]
    );
}

#[test]
fn cycles_and_import_prefixes_are_handled() {
    let project = project(&[
        (
            "lib/cycle.dart",
            "class A extends B {}\nclass B extends A {}\nclass C extends A {}\n",
        ),
        (
            "lib/prefixed.dart",
            "import 'package:flutter/widgets.dart' as w;\nimport 'base.dart' as b;\nclass Screen extends b.Base {}\nclass Direct extends w.StatelessWidget {}\n",
        ),
        (
            "lib/base.dart",
            "import 'package:flutter/widgets.dart';\nclass Base extends InheritedWidget {}\n",
        ),
    ]);

    let inventory = extract_flutter_inventory(&project);

    assert_eq!(
        widgets(&inventory),
        [
            "lib/base.dart:Base<-InheritedWidget",
            "lib/prefixed.dart:Screen<-InheritedWidget via b.Base",
            "lib/prefixed.dart:Direct<-w.StatelessWidget",
        ]
    );
}

#[test]
fn a_mixin_does_not_make_a_widget_but_the_superclass_does() {
    let project = project(&[(
        "lib/mixins.dart",
        concat!(
            "import 'package:flutter/widgets.dart';\n",
            "mixin Highlight on StatelessWidget {}\n",
            "class NotAWidget with Highlight {}\n",
            "class Base extends StatelessWidget {}\n",
            "class Mixed extends Base with Highlight {}\n",
            "class Applied = Base with Highlight;\n",
        ),
    )]);

    let inventory = extract_flutter_inventory(&project);

    assert_eq!(
        widgets(&inventory),
        [
            "lib/mixins.dart:Base<-StatelessWidget",
            "lib/mixins.dart:Mixed<-StatelessWidget via Base",
            "lib/mixins.dart:Applied<-StatelessWidget via Base",
        ]
    );
}

#[test]
fn the_json_names_the_class_a_widget_is_reached_through_only_when_there_is_one() {
    let project = project(&[(
        "lib/screens.dart",
        "class Base extends StatelessWidget {}\nclass Screen extends Base {}\n",
    )]);

    let json = serde_json::to_string(&extract_flutter_inventory(&project)).expect("serialize");

    assert_eq!(json.matches("\"inherited_via\"").count(), 1, "{json}");
    assert!(json.contains("\"inherited_via\":\"Base\""), "{json}");
    // Output of earlier versions has no such field.
    let without = json.replace(",\"inherited_via\":\"Base\"", "");
    let reread: FlutterInventory = serde_json::from_str(&without).expect("deserialize");
    assert_eq!(reread.widgets[1].inherited_via, None);
}
