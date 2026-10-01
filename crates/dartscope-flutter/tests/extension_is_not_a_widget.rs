//! An `extension ... on Widget` extends a type, not a class, so it is not a Flutter widget.
//! (Found by the 2026-09-30 audit: the extension's `on` type used to be reported as `extends`.)

use dartscope_core::DartFileInput;
use dartscope_flutter::derive_flutter_file_hints;
use dartscope_parse::analyze_file;

#[test]
fn extension_on_widget_is_not_a_flutter_widget() {
    let source = "import 'package:flutter/widgets.dart';\nextension WidgetX on Widget {\n  Widget padded() => this;\n}\nextension on StatelessWidget {}\nclass Real extends StatelessWidget {}\n";
    let file = analyze_file(DartFileInput::new("lib/x.dart", source));
    let hints = derive_flutter_file_hints(&file);
    let names: Vec<String> = hints
        .widgets
        .iter()
        .map(|w| format!("{:?}<-{}", w.class_name, w.base_class))
        .collect();
    println!("spec F1 widget hints: {names:?}");
    assert_eq!(
        hints.widgets.len(),
        1,
        "extensions reported as widgets: {names:?}"
    );
}
