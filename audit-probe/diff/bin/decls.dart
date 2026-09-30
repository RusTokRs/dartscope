// Reference extractor: lists declarations and directives using the official Dart parser (package:analyzer).
import 'dart:convert';
import 'dart:io';

import 'package:analyzer/dart/analysis/features.dart';
import 'package:analyzer/dart/analysis/utilities.dart';
import 'package:analyzer/dart/ast/ast.dart';

void main(List<String> args) {
  final paths = File(args[0]).readAsLinesSync().where((l) => l.trim().isNotEmpty);
  final out = <Map<String, Object?>>[];
  final featureSet = FeatureSet.latestLanguageVersion();
  for (final path in paths) {
    String text;
    try {
      text = File(path).readAsStringSync();
    } catch (e) {
      out.add({'f': path, 'err': 'read: $e'});
      continue;
    }
    ParseStringResult result;
    try {
      result = parseString(content: text, featureSet: featureSet, path: path, throwIfDiagnostics: false);
    } catch (e) {
      out.add({'f': path, 'err': 'parse: $e'});
      continue;
    }
    final unit = result.unit;
    final lines = result.lineInfo;
    out.add({'f': path, 'syntax_errors': result.errors.length});

    void add(String kind, String name, int offset, {String owner = ''}) {
      out.add({'f': path, 'k': kind, 'n': name, 'o': owner, 'l': lines.getLocation(offset).lineNumber});
    }

    void members(List<ClassMember> list, String owner) {
      for (final m in list) {
        if (m is MethodDeclaration) {
          final kind = m.isGetter ? 'getter' : m.isSetter ? 'setter' : m.isOperator ? 'operator' : 'method';
          add(kind, m.name.lexeme, m.offset, owner: owner);
        } else if (m is ConstructorDeclaration) {
          final n = m.name;
          add('constructor', n == null ? owner : '$owner.${n.lexeme}', m.offset, owner: owner);
        } else if (m is FieldDeclaration) {
          for (final v in m.fields.variables) {
            add('field', v.name.lexeme, v.offset, owner: owner);
          }
        }
      }
    }

    for (final d in unit.declarations) {
      if (d is ClassDeclaration) {
        add('class', d.name.lexeme, d.offset);
        members(d.members, d.name.lexeme);
      } else if (d is MixinDeclaration) {
        add('mixin', d.name.lexeme, d.offset);
        members(d.members, d.name.lexeme);
      } else if (d is EnumDeclaration) {
        add('enum', d.name.lexeme, d.offset);
        for (final c in d.constants) {
          add('enum_constant', c.name.lexeme, c.offset, owner: d.name.lexeme);
        }
        members(d.members, d.name.lexeme);
      } else if (d is ExtensionDeclaration) {
        add('extension', d.name?.lexeme ?? '', d.offset);
        members(d.members, d.name?.lexeme ?? '');
      } else if (d is ExtensionTypeDeclaration) {
        add('extension_type', d.name.lexeme, d.offset);
        members(d.members, d.name.lexeme);
      } else if (d is FunctionDeclaration) {
        final kind = d.isGetter ? 'getter' : d.isSetter ? 'setter' : 'function';
        add(kind, d.name.lexeme, d.offset);
      } else if (d is TopLevelVariableDeclaration) {
        for (final v in d.variables.variables) {
          add('variable', v.name.lexeme, v.offset);
        }
      } else if (d is GenericTypeAlias) {
        add('typedef', d.name.lexeme, d.offset);
      } else if (d is FunctionTypeAlias) {
        add('typedef', d.name.lexeme, d.offset);
      } else if (d is ClassTypeAlias) {
        add('class', d.name.lexeme, d.offset);
      }
    }
    for (final d in unit.directives) {
      if (d is ImportDirective) {
        out.add({'f': path, 'k': 'import', 'n': d.uri.stringValue ?? '', 'l': lines.getLocation(d.offset).lineNumber});
      } else if (d is ExportDirective) {
        out.add({'f': path, 'k': 'export', 'n': d.uri.stringValue ?? '', 'l': lines.getLocation(d.offset).lineNumber});
      } else if (d is PartDirective) {
        out.add({'f': path, 'k': 'part', 'n': d.uri.stringValue ?? '', 'l': lines.getLocation(d.offset).lineNumber});
      }
    }
  }
  File(args[1]).writeAsStringSync(jsonEncode(out));
}
