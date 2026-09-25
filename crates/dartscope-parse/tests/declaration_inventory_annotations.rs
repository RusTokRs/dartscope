use dartscope_core::{DartDeclarationKind, DartFileAnalysis, DartFileInput};
use dartscope_parse::analyze_file;

fn analyze(source: &str) -> DartFileAnalysis {
    analyze_file(DartFileInput::new("lib/sample.dart", source))
}

fn kinds(analysis: &DartFileAnalysis) -> Vec<(&str, DartDeclarationKind)> {
    analysis
        .declarations
        .iter()
        .map(|declaration| (declaration.name.as_str(), declaration.kind))
        .collect()
}

#[test]
fn annotation_sharing_a_line_with_the_declaration_is_skipped() {
    let analysis = analyze(
        "@Deprecated('x') void sameLine() {}\n\
         @Deprecated('y') class Annotated {}\n\
         @Deprecated('z') const int value = 1;\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![
            ("sameLine", DartDeclarationKind::Function),
            ("Annotated", DartDeclarationKind::Class),
            ("value", DartDeclarationKind::Variable),
        ]
    );
    assert_eq!(analysis.declarations[0].span.start_line, 1);
}

#[test]
fn annotated_members_and_locals_are_collected() {
    let analysis = analyze(
        "class A {\n\
           @override int get x => 1;\n\
           @Deprecated('z') final int z = 3;\n\
           void m() {\n\
             @Deprecated('l') final int local = 1;\n\
           }\n\
         }\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![
            ("A", DartDeclarationKind::Class),
            ("x", DartDeclarationKind::Getter),
            ("z", DartDeclarationKind::Field),
            ("m", DartDeclarationKind::Method),
            ("local", DartDeclarationKind::LocalVariable),
        ]
    );
}

#[test]
fn annotation_arguments_may_span_lines_before_the_declaration() {
    let analysis = analyze(
        "@Deprecated(\n\
           'tail',\n\
         ) void tail() {}\n\
         @Foo.bar<int>('q')\n\
         void generic() {}\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![
            ("tail", DartDeclarationKind::Function),
            ("generic", DartDeclarationKind::Function),
        ]
    );
}

#[test]
fn multi_line_initializer_arrow_does_not_fabricate_declarations() {
    let analysis = analyze(
        "final GoRouter appRouter = GoRouter(\n\
           routes: [\n\
             GoRoute(\n\
               path: homeRoute,\n\
               builder: (context, state) => const HomeScreen(),\n\
             ),\n\
           ],\n\
         );\n\
         void after() {}\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![
            ("appRouter", DartDeclarationKind::Variable),
            ("after", DartDeclarationKind::Function),
        ]
    );
}

#[test]
fn commented_out_declarations_are_ignored() {
    let analysis = analyze(
        "// void commentedOut() {}\n\
         /* class AlsoCommented {} */\n\
         void real() {}\n",
    );

    assert_eq!(
        kinds(&analysis),
        vec![("real", DartDeclarationKind::Function)]
    );
}
