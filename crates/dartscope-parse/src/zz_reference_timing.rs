//! TEMPORARY development-loop measurement (removed before hand-off): which reference pass is
//! superlinear in the number of declarations of one file.
use std::time::Instant;

use dartscope_core::DartFileInput;

use crate::identifier_references::{collect_identifier_references, sort_identifier_references};
use crate::lexical::mask_non_code;
use crate::lexical_bindings::collect_lexical_bindings;
use crate::lexical_reads::collect_lexical_read_references;
use crate::lexical_writes::{collect_lexical_update_references, collect_lexical_write_references};
use crate::member_references::collect_method_references;
use crate::operator_references::collect_operator_references;
use crate::property_references::collect_property_references;
use crate::source_lines::LineIndexScope;

fn source(classes: usize) -> String {
    let mut source = String::new();
    for class in 0..classes {
        let base = if class > 0 {
            format!("C{}", class - 1)
        } else {
            "Object".to_string()
        };
        source.push_str(&format!(
            "class C{class} extends {base} {{\n  int a{class} = 0;\n  int m{class}(int x) {{\n    this.a{class} = x;\n    return this.m0(x) + this.a{class};\n  }}\n  void run{class}() {{\n    this.run0();\n    this.m{class}(1);\n    this.missing();\n    helper();\n  }}\n}}\n"
        ));
    }
    source.push_str("int helper() => 1;\n");
    source
}

#[test]
#[ignore = "development-loop measurement"]
fn zz_reference_timing() {
    for classes in [500usize, 1000, 2000, 4000] {
        let source = source(classes);
        let t = Instant::now();
        let file = crate::analyze_file(DartFileInput::new("lib/a.dart", source.clone()));
        let t_file = t.elapsed();
        let _lines = LineIndexScope::enter(&source);
        let lexical = mask_non_code(&source);
        let t = Instant::now();
        let bindings = collect_lexical_bindings(&source, &lexical.code, &file);
        let t_bindings = t.elapsed();
        let t = Instant::now();
        let mut references = collect_identifier_references(&source, &lexical.code, &file, &bindings);
        let t_identifiers = t.elapsed();
        let t = Instant::now();
        let reads =
            collect_lexical_read_references(&source, &lexical.code, &file, &bindings, &references);
        references.extend(reads);
        let t_reads = t.elapsed();
        let t = Instant::now();
        let writes =
            collect_lexical_write_references(&source, &lexical.code, &file, &bindings, &references);
        references.extend(writes);
        let t_writes = t.elapsed();
        let t = Instant::now();
        let updates =
            collect_lexical_update_references(&source, &lexical.code, &file, &bindings, &references);
        references.extend(updates);
        let t_updates = t.elapsed();
        let t = Instant::now();
        references.extend(collect_method_references(&source, &lexical.code, &file, &bindings));
        let t_methods = t.elapsed();
        let t = Instant::now();
        references.extend(collect_property_references(&source, &lexical.code, &file, &bindings));
        let t_properties = t.elapsed();
        let t = Instant::now();
        references.extend(collect_operator_references(&source, &lexical.code, &file));
        let t_operators = t.elapsed();
        let t = Instant::now();
        sort_identifier_references(&mut references);
        let t_sort = t.elapsed();
        println!(
            "phase refs classes={classes} decls={} refs={} file={t_file:?} bindings={t_bindings:?} identifiers={t_identifiers:?} reads={t_reads:?} writes={t_writes:?} updates={t_updates:?} methods={t_methods:?} properties={t_properties:?} operators={t_operators:?} sort={t_sort:?}",
            file.declarations.len(),
            references.len()
        );
    }
}
