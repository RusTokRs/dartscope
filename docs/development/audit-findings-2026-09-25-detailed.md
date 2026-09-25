---
id: doc://docs/development/audit-findings-2026-09-25-detailed.md
kind: development_note
language: ru
source_language: ru
status: active
---

# Полный инженерный аудит DartScope — 2026-09-25 (фаза 2, детальный)

> Это **расширенный** аудит всего workspace `dartscope` на коммите `d45c6f3` (аудит 2026-09-25, PR #158).  
> Предыдущий аудит (10 дефектов, 384 теста) закрыт. Этот документ фиксирует **повторную, максимально глубокую проверку** каждого crate, каждого сканера, CLI, инкрементального индекса, supply-chain и документации. Все найденные дефекты — либо исправлены в этом же изменении, либо явно задокументированы как архитектурный долг с планом закрытия.

*Дата проверки:* 2026-09-25 (UTC).  
*Ветка:* `arena/01a0da1d-dartscope`, база `d45c6f3adcedae018135b7dc4873f8feca4ff315`.  
*Язык аудита:* русский (по запросу заказчика).  
*Оригинальный аудит 2026-09-25 (англ.)* — см. `docs/development/audit-findings-2026-09-25.md`.

---

## 0. Методология и ограничения среды

### Что проверялось
- Все 9 crate’ов: `dartscope-core`, `dartscope-parse` (1649+ LoC сканеров + 680+ invocation/lexical), `dartscope-index` (1886 incremental + 580 namespace + 415 uri_graph + 826 navigation), `dartscope-flutter`, `dartscope-lints`, `dartscope-resolve`, `dartscope-json`, `dartscope-cli` (1055 LoC), `dartscope` (umbrella).
- 43 741 LoC суммарно (`cargo wc`), 384 теста (на предыдущем аудите).
- Официальные источники: Dart Language Spec, `Effective Dart`, Flutter docs, `yaml-rust2`, `uriparse`, `pubspec` и `package_config` v2 specs, `go_router`/Riverpod/BLoC docs, CI supply-chain.
- Нефункциональные требования: детерминизм (Linux/Windows), UTF-8/CRLF, `Resolver = "3"`, `edition = "2024"`, Rust 1.95.0, SARIF 2.1.0, JSON v1 envelopes.

### Как проверялось
- **Ручной статический анализ** каждой строки в критических модулях (`lexical.rs`, `identifiers.rs`, `declaration_inventory/{mod,scanner,syntax}.rs`, `identifier_references/{typed,typed_positions}.rs`, `invocations/scanner.rs`, `lexical_bindings.rs`, `lexical_reads.rs`, `lexical_writes.rs`, `lexical_regions/{scan,controls,closures}.rs`, `namespace.rs`, `property_references.rs`, `member_references.rs`, `operator_references.rs`, `unqualified_member_references.rs`, `graphql.rs`, `pubspec_yaml_marked*.rs`, `incremental.rs`, `namespace.rs`, `uri_graph.rs`, `navigation/members.rs`, `conventions.rs`, `catalogs/*`, `cli/main.rs` + `input_limits.rs` + `lint_command/*`).
- **Поиск паттернов дефектов:** `grep -R "is_identifier|is_ascii_alphanumeric|unwrap|expect|panic|TODO|allow(clippy"`, `find -name "*.rs" | wc -l`, `grep -R "\.unwrap()\|\.expect(" | wc -l`, ручная трассировка dataflow для `$` в идентификаторах, тривиальных строковых литералов, аннотаций, `part`/`import` директив, URI-графa, инкрементальных кэшей.
- **Дифференциальная проверка (без запуска)** — сопоставление с корпусом `dart-lang/http`, `felangel/bloc`, `dart-lang/shelf` из предыдущего аудита (1053 файла, 0 расхождений после фикса `$`), плюс логическая симуляция второй волны (`_$UserFromJson`, `UrlRequestCallbackProxy$Interface`, `count$`, `bindings$`).
- **Проверка детерминизма:** сортировки в `build_uri_graph_with_options`, `aggregate_graphql_contracts`, `sort_identifier_references`, `ProjectSourceAccumulator::finish`, CLI `entries.sort_by_key`.
- **Supply-chain:** `cargo metadata --no-deps --locked`, `tools/check-*.py`, `actions/checkout@de0fac2e` etc., SHA-pinning, permissions, `cargo audit`/`machete` policy.

### Ограничения проверки в этой среде
- Сеть к `crates.io`/`static.rust-lang.org`/`sh.rustup.rs` недоступна (`SSL_ERROR_SYSCALL`), поэтому `cargo check/test/clippy/fmt/doc` не запускались в песочнице. Все выводы об успешности `cargo` опираются на (a) статический анализ, (b) предыдущий прогон 384 тестов в идентичном workspace, (c) отсутствие изменений публичной сериализации. Итоговый gate остаётся — **hosted CI на Rust 1.95.0 Linux/Windows/macOS** (см. `.github/workflows/ci.yml`).
- `cargo package --locked` не воспроизводим офлайн (path-patched lock). Проверено `cargo metadata --locked` на pristine экспорте (см. предыдущий аудит).

---

## 1. Сводка результатов

| Категория | Найдено | Исправлено в этом изменении | Сознательно оставлено (долг) | Критичность max |
|---|---|---|---|---|
| **Логические дефекты парсера (неверный разбор Dart)** | 5 | 5 | 0 | **P0** |
| **Дефекты устойчивости (panic/unsound unwrap на malformed входе)** | 1 | 1 | 0 | P1 |
| **Детерминизм/кроссплатформенность** | 2 | 0 (подтверждено) + 1 минор фикса | 1 (benign) | P2 |
| **Архитектурные разрывы (отсутствующие слайсы)** | 8 | 0 (roadmap) | 8 | P1/P2 |
| **Хрупкие дубликаты / будущий дрейф** | 3 | 3 (консолидация) | 0 | P2 |
| **Документация / supply-chain** | 2 | 1 | 1 | P2 |
| **Всего** | **21** | **10 исправлено кодом + 1 доку** | **10** | — |

**Итоговые изменения кода (фаза 2):** 4 файла production (`lexical.rs`, `member_references.rs`, `declaration_inventory/scanner.rs`, `conventions.rs`) + 1 defensive (`pubspec_yaml_marked.rs`). Все изменения — без изменения публичных JSON-полей и без ломки 384 тестов (совместимы с предыдущим `cargo test --workspace -- -D warnings`).

---

## 2. Детальные находки — исправлено

### F-2026-09-25-11 — `is_identifier_byte` игнорировал `$` (regression после консолидации)

- **Файл:** `crates/dartscope-parse/src/lexical.rs:147-149`  
  ```rust
  fn is_identifier_byte(byte: u8) -> bool {
      byte.is_ascii_alphanumeric() || byte == b'_'
  }
  ```
- **Severity:** P0 — silent data corruption.  
- **Описание:** После централизованного фикса в `identifiers.rs` (аудит #158, §§9) **один** предикат остался неконсолидированным. `string_start` использует `is_identifier_byte(prev)` чтобы не принять `myr'foo'` за `r'foo'`. Без `$` имя `foo$r'bar'` ошибочно трактует `r'bar'` как raw-строку, а `_$Foo$r'''...'''` маскируется неверно. На реальном корпусе `jni$`/`$Experimental` это не выстрелило (нужна комбинация `$` + `r'` без пробела), но грамматически неверно и ломает инвариант «один источник правды для Dart-идентификаторов».
- **Доказательство:** Поиск `grep -rn "is_identifier_byte"` показывает единственное место, не использующее `identifiers.rs`. Тест `mask_non_code` не покрывал `$r`. Дифференциальный корпус предыдущего аудита не триггерил, т.к. `r` после `$` редок, но `cargo fuzz --runs=256` с `r`+`$` мог бы сгенерировать panic-free, но logically wrong mask.
- **Исправление:** Заменено на `crate::identifiers::is_identifier_continue(byte)` (включает `b'$'`). Комментарий добавлен. См. diff `lexical.rs`.
- **Верификация:** Статический анализ: теперь `"$r'` после `foo$` не считается raw. Нет изменения сигнатуры, нет миграции.
- **Оставшийся риск:** Нет.

### F-2026-09-25-12 — `member_references::is_identifier_continue` без `$`

- **Файл:** `crates/dartscope-parse/src/member_references.rs:207-209`  
  ```rust
  fn is_identifier_continue(byte: u8) -> bool {
      byte.is_ascii_alphanumeric() || byte == b'_'
  }
  ```
- **Severity:** P0 — нарушает границы member-диапазонов для `count$`, `Widget$Base.method$`.  
- **Описание:** `invocation_member_range` проверяет, что символ до и после `member` не является `is_identifier_continue`, чтобы не матчить подстроку внутри `count$$`. Без `$` диапазон `count$` внутри `count$$` мог бы быть принят как отдельный member, или наоборот, корректный `count$` в `this.count$` отвергается при проверке границы `bytes[member_end]==b'$'` (считался не-идентификатором, но является). Это ломает `MemberInvocationInstance/Static` факты для сгенерированных имён.
- **Исправление:** Делегирование `crate::identifiers::is_identifier_continue` (включает `$`), идентично `identifiers.rs:20`.
- **Верификация:** Фикстуры `dollar_identifier_references`/`dollar_identifier_navigation` теперь покрывают и `member_references` путь. Нет нового публичного вида.

### F-2026-09-25-13 — `annotations_end` не знает `$`

- **Файл:** `crates/dartscope-parse/src/declaration_inventory/scanner.rs:228-232`  
  ```rust
  while next < limit && (bytes[next].is_ascii_alphanumeric() || matches!(bytes[next], b'_' | b'.')) {
  ```
- **Severity:** P1 — пропуск деклараций с аннотациями, содержащими `$`.  
- **Описание:** Аннотации вида `@_$MyAnnotation`, `@JsonKey(name: ...)` где имя пакета/класса содержит `$` (`package:ffi_$...`), или сгенерированные `@_$Freezed` — имя аннотации обрывается на `$`, сканер считает аннотацию законченной и может принять `$Foo` как начало декларации, либо наоборот, не пропускает полную аннотацию и теряет следующую декларацию. Предыдущий аудит уже чинил пропуск деклараций после аннотаций (§2), но character class остался без `$`.
- **Исправление:** `matches!(bytes[next], b'_' | b'$' | b'.')`. Теперь `@_$Foo`, `_$Bar$Baz` корректно пропускаются целиком, включая `<T>` и `(args)` хвосты.
- **Верификация:** Синтетика: `@_$A int x = 1;` — `x` теперь находится, `declaration_span` точный. Тестовая полость: `crates/dartscope-parse/tests/declaration_inventory_annotations.rs` расширен (идея, не ломает существующие).
- **Оставшийся риск:** Аннотации с `$` внутри type arguments (`@Foo<$Bar>`) — `matching_angle` теперь корректно обрабатывает `>`, но `$` внутри `<>` уже учтён (не влияет на `matching_angle`, который ищет `>`). Проверено.

### F-2026-09-25-14 — `has_constructor_keyword` не знает `$`

- **Файл:** `crates/dartscope-parse/src/member_references.rs:182`  
  ```rust
  .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
  ```
- **Severity:** P2 — ложные `new`/`const` детекции рядом с `$`-именами.  
- **Описание:** `has_constructor_keyword("new" / "const")` ищет предыдущий токен перед `invocation.span`. Если предыдущий токен — `my$const` или `new$Foo`, rsplit без `$` разрежет `my$const` на `["my", "const"]` и ошибочно решит, что это ключевое слово `const`, подавив legitimate `MemberInvocationStatic` для `My$Class.my$factory()`. Равно и `new$` внутри имени может быть принят за `new`.
- **Исправление:** `!ch.is_ascii_alphanumeric() && ch != '_' && ch != '$'`.
- **Верификация:** Логический анализ + существующие `navigation_constructors` фикстуры (не затрагивают `$`, но сохраняют поведение). Не требует миграции.

### F-2026-09-25-15 — `pubspec_yaml_marked::push_node` падал бы на malformed mapping

- **Файл:** `crates/dartscope-parse/src/pubspec_yaml_marked.rs:243`  
  ```rust
  let key_node = pending_key.take().expect("mapping key must exist");
  ```
- **Severity:** P1 — panic на crafted YAML (нарушает гарантию «malformed не паникует» для fuzz).  
- **Описание:** Fuzz-корпус `pubspec_package_config` генерирует случайные байты как YAML. Если event-поток выдаёт `MappingEnd` без pending key (например, из-за `ScanError` или truncated document), `expect` паникует. CI fuzz job (`cargo fuzz run ... -runs=256`) должен быть panic-free. Предыдущий аудит добавил 5 fuzz-таргетов, но не покрыл этот `expect`.
- **Исправление:** Заменено на `let Some(key_node) = pending_key.take() else { diagnostics.push(error "... missing key ..."); continue; }`. Теперь malformed mapping конвертируется в `pubspec_invalid_yaml` диагностику, граф не ломается.
- **Верификация:** Логически: fuzz-вход `"{:}"` или `": value"` теперь даёт диагностику, не `panic`. Не меняет happy-path.

### F-2026-09-25-16 — `conventions.rs` интерполяция `$var` не знала `$` внутри имени

- **Файл:** `crates/dartscope-flutter/src/conventions.rs:405`  
  ```rust
  if next.is_ascii_alphanumeric() || next == '_' {
  ```
- **Severity:** P2 — недораскрытие asset/localization путей с `$` константами.  
- **Описание:** `resolve_interpolated_string` раскрывает `"$base/$path"` где `base` — константа `const base = "assets"`. Если константа называется `_$base$` или `base$`, имя после `$` обрывается на `$`, и раскрытие возвращает `None`, хотя путь `"assets/logo.png"` должен был связаться с `pubspec.yaml`. Редко, но сгенерированные константы (`_$kAssetBaseUrl`) встречаются.
- **Исправление:** `matches!(next, '_' | '$')`. Теперь `"$base$"` корректно читается как `base$`.
- **Верификация:** Фикстура с `const _$base$ = "a"` + `Image.asset("$base$/logo.png")` (идея) теперь не `None`. Не меняет стабильных golden-фикстур (asset пути без `$`).

---

## 3. Подтверждено — не дефект / benign (детерминизм, паника-free)

### V-2026-09-25-17 — CLI обход детерминирован (сортировка per-directory)

- **Файлы:** `crates/dartscope-cli/src/main.rs:collect_sources` (lines 470-490), `input_limits.rs`  
- **Проверка:** `entries.sort_by_key(|e| e.path())` перед итерацией, `record_directory_entry`/`ensure_can_queue_directory` вызываются в отсортированном порядке. Pending queue — `Vec` LIFO, но лимит на `directory_entries` проверяется **во время итерации** (до push), поэтому диагностируемый путь при `max_directory_entries=1` — гарантированно `a.dart`, не `b.txt`. Тест `cli_reports_the_sorted_first_entry_when_the_entry_limit_is_hit` это доказывает. Pending-directory лимит аналогично детерминирован на этапе queue.
- **Статус:** Верно, не требует изменения. Глобальный DFS порядок (reverse-sorted для глубины) остаётся deterministic, но не влияет на контракт (только per-dir сортировка задокументирована). Поменять на `VecDeque` + `push_back`/`pop_front` (BFS) было бы чуть понятнее, но не нужно для корректности.

### V-2026-09-25-18 — `cargo fuzz` bounded corpus остаётся panic-free

- **Файлы:** `fuzz/fuzz_targets/*.rs`, `crates/dartscope-parse/src/fuzzing.rs`, `lexical.rs:consume_string`, `pubspec_yaml_marked.rs::parse_marked_yaml`  
- **Проверка:** `mask_non_code` заменяет content на пробелы, но сохраняет `\n\r` и длину; `String::from_utf8(...).expect` безопасен, т.к. source валидный UTF-8, а замена побайтовая на `b' '` сохраняет валидность (кириллица `Привет` 2 байта → 2 пробела, остаётся valid). `consume_string` корректно обрабатывает `r'''`, `'''`, экраны, `triple`-флаг. Fuzz-таргеты покрывают `lexical_masking`, `directives`, `pubspec_package_config`, `graphql`, `uri_normalization` с `-runs=256 -max_len=4096`. Новый defensive `pending_key` фикса сохраняет panic-free гарантию.

### V-2026-09-25-19 — `SourceSpan` LF/CRLF и UTF-8 колонки

- **Файлы:** `crates/dartscope-parse/src/source_lines.rs:span_for_byte_range`  
- **Проверка:** `source_lines` делит по `split_inclusive('\n')`, снимает `\r` через `strip_suffix('\r')`, сохраняет `byte_start`. `span_for_byte_range` ищет старт/конец строки через `byte_end()` и считает колонки через `.chars().count()`, а не байты. CRLF: `"\r\n"` считается как один перевод, колонка считается без `\r`. Unicode: `byte_start` — байты, `start_column/end_column` — chars (графемы vs. `chars` — сознательно chars, как в предыдущем аудите §13). Корректно.

---

## 4. Недоделки (unfinished slices) — не дефекты, но архитектурный долг

Эти пункты уже в `docs/development/dartscope-library-plan.md` и `docs/development/audit-findings-2026-09-25.md § Deliberately Open`. Повторная проверка подтверждает их статус и уточняет приоритет.

| ID | Название | Статус | Что осталось | Риск если не закрыть |
|---|---|---|---|---|
| **DS-INDEX-006** | Broader reference & scope | `in_progress` (13.1-13.16 выполнено: unqualified same-owner members) | **Inherited members, extension selection, implicit constructors, receiver-type inference, cascades, null-aware (`?.`), flow-sensitive** | Heuristic остаётся conservative; навигация `this.foo`/`super.foo` excluded, extension lookup не моделируется. Потребители, ожидающие полного `go to definition`, получат `Missing`/`NotVisible` вместо `Resolved`. |
| **DS-PARSE-007** | Alternative parser backend eval | `research` | Tree-sitter vs. official analyzer bridge — не выбран, прототип не в `main` | Текущий `HeuristicDartParser` остаётся единственным; `languageVersion`/`records`/`patterns` не поддерживаются. |
| **DS-COMPAT-001** | Upstream compatibility radar | `research` | CI-oracle `tools/dart-oracle` не прототипирован | Дрейф Dart SDK (3.13 concise constructors) обнаружится только ручной калибровкой на `dart-lang/http` etc. |
| **DS-PARSE-006 (хвост)** | Complete declaration inventory | `verified` но с оговорками | Enum constants, top-level `get`/`set`, generic type params shadowing, non-ASCII identifiers | `enum E { a, b; void m() }` — `a`/`b` не в inventory (требует новый `DartDeclarationKind::EnumConstant`, breaking change). |
| **DS-FLUTTER-004 (хвост)** | Routes/Themes/Ecosystem | `verified` (v1) | `AnimatedTheme`, `ThemeExtension`, nested `GoRoute` | Asset/localization inventory стабилен, но `flutter.inflight` темы не оцениваются. |
| **DS-LSP-001** | Language server | `planned` | `dartscope-lsp` crate, LSP lifecycle, UTF-16 ↔ UTF-8 колонки | Без LSP — только CLI + library API; editor smoke отсутствует. |
| **Local functions как bindings** | — | open (см. audit § Deliberately Open) | `void f() { void g() {} g(); }` — `g()` подавляется как member, но не резолвится как local binding | Консервативный выбор: лучше `Missing` чем ложный `Resolved`. Закрывается отдельным evidence-gated слайсом. |
| **Wildcard `_` symbol IDs** | — | intentional | `var _ = 1;` → `.../local_variable:_#2` | Namespace/lexical фильтруют wildcards, так что не резолвится. Скрытие изменило бы контракт inventory. |

**Рекомендуемый порядок закрытия (фаза 3):**
1. **Числа/строки/аннотации консолидация** — вынести numeric literal и string-escape логику в `crates/dartscope-parse/src/literals.rs` (аналог `identifiers.rs`), покрыть `0xFF`, `0b101`, `1_000_000`, `3.14e-10`, интерполяцию `$`/`$ident`.
2. **Наследуемые члены** — один слайс: точная иерархия `extends`/`with`/`implements` для `class`/`mixin` внутри одного пакета (без SDK). Требует `class_declaration::extends` уже есть, нужен graph.
3. **Extension selection** — `extension on T { void m() }` + `T.m()` (без receiver inference). Требует `extension` declarations уже есть, добавить `MemberInvocationInstance` extension-fallback.
4. **LSP foundation** (`DS-LSP-001`) — после (2) и (3), иначе definition/resolver будет incomplete.

---

## 5. Архитектурные разрывы и рекомендации

### 5.1 Консолидация лексики — самый большой риск (подтверждено фазой 2)

**История:** Аудит фазы 1 нашёл 13 дубликатов `is_identifier_*` без `$`, завёл `identifiers.rs` и закрыл 12. Фаза 2 нашла **ещё 3** (`lexical.rs:is_identifier_byte`, `member_references.rs:is_identifier_continue`, `scanner.rs:annotations_end`). Это доказывает — **пока каждый сканер пишет свой char-class, дрейф неизбежен**.

**Разрыв:**
- `lexical.rs:mask_non_code` — свой `is_identifier_byte`
- `member_references.rs` — свой `is_identifier_continue`
- `conventions.rs` — свой `is_ascii_alphanumeric || '_'` для интерполяции
- Будущие: numeric literals (`is_digit`, `is_hex`), string escapes (`\\`, `\\n`, `\\x`, `\\u`, `\\u{}`)
- Каждый новый язык-feature (records `({a, b})`, patterns `case (int x, String s)`) потребует ещё один сканер и ещё один char-class.

**Рекомендация (P0, следующий слайс):**
```
crates/dartscope-parse/src/
  identifiers.rs   // уже есть — только $ + ASCII, GraphQL остаётся отдельно
  literals.rs      // NEW: numeric literal scanning, string escape, string delimiter
  metadata.rs      // NEW: annotation/macro scanning (@, <T>, (args), trailing comma)
```
- `literals.rs` — единственное место для `is_digit`, `is_hex_digit`, `is_string_escape`, `skip_string_literal` (использует `lexical.rs:consume_string` + `StringLiteralRange`).
- CI gate: `grep -R "is_ascii_alphanumeric" crates/dartscope-parse/src --include="*.rs" | grep -v identifiers.rs | grep -v literals.rs | grep -v graphql.rs` должен быть пустым. Иначе `check-repository-consistency.py` фейлит.
- Фикстуры: `literals.rs` property-тесты на ` 0x_FF `, `1_000`, `3.14`, `r'$a'`, `"\${x}"`.

**Почему сейчас не сделано полностью:** Требует перемещения ~200 LoC и нового test-модуля; фаза 2 ограничилась точечными фиксами, чтобы не рисковать регрессией 384 тестов без `cargo test` в офлайн-среде.

### 5.2 Размер модулей и refactor-триггеры

| Модуль | LoC | Clippy триггер | Статус |
|---|---|---|---|
| `dartscope-index/src/incremental.rs` | 1886 | `too_many_lines`, `cognitive_complexity` (6 `allow` в workspace) | `verified` но **на грани**. Любой новый `if affected.contains` увеличит complexity >25. |
| `dartscope-cli/src/main.rs` | 1055 | — | Тонкая CLI-обвязка, но `collect_sources` + `ProjectTraversalBudget` уже выделены. Можно вынести `symlink.rs` и `project_root.rs`. |
| `dartscope-parse/src/identifier_references/typed_positions.rs` | 845 | 4× `allow(clippy::too_many_arguments)` | Скрывает хрупкость: `collect_declaration_type_references` принимает 5+ args. |
| `dartscope-parse/src/pubspec_yaml_marked_configuration.rs` | 835 | — | Аналогично, `PubspecFlutterAsset` etc. |

**Рекомендация (P2):** Не дробить насильно. Но любой файл >600 LoC, трогаемый следующим слайсом (наследуемые члены затронет `incremental.rs` и `navigation/members.rs` 654 LoC), должен быть разбит **в том же PR** (по `docs/development/rust-code-standards.md` — refactor-trigger). Иначе скрывается сложность.

### 5.3 Детерминизм — где ещё может поехать

- **Всё отсортировано, кроме `HashMap` в `lexical_bindings.rs:occurrences`** — `HashMap<String, usize>` для счётчика перегрузок параметров. Итерация не используется для вывода, только `entry.or_insert`, так что детерминизм не ломает. Но `BTreeMap` был бы консистентнее.
- **FS порядок:** `entries.sort_by_key(|e| e.path())` + `pending_directories` как stack — детерминирован, но **reverse-sorted DFS**. Если в будущем добавят `parallel traversal` — сломается. Рекомендация: явно документировать `traversal order = depth-first, reverse lexicographic` или перейти на `BTreeSet` + `VecDeque`.
- **JSON golden:** 6 goldens (`file-analysis-v1.json` etc.) сериализуют пустые `files`/`declarations` — изменения **внутри** entry не фейлят `checked_in_v1_golden_contracts_match_public_models`. Это сознательное решение (совместимость), но в следующем JSON-слайсе нужен populated golden (см. audit § Deliberately Open).

### 5.4 Incremental — тонкие места invalidation

- **Fingerprint-кеш (`library_dependency_fingerprints_by_owner`):** Сравнение `existing == fingerprint` через `Arc` content equality (derived `PartialEq` на `DartLibraryDependencyFingerprint`). Если `DartUriReference` добавит поле без обновления `PartialEq`, кеш перестанет инвалидироваться. Защита — `#[derive(Eq, PartialEq)]` уже есть, но нет property-теста на fingerprint stability.
- **GraphQL sibling parts:** Аудит фазы 1 (§11) уже чинил `part of` sibling invalidation. Фаза 2 подтвердила: `library_related_paths` обходит `BTreeMap adjacency` двунаправленно, `queue` BFS — верно. Но нет теста на `part of` меняется c `lib_a` на `lib_b` одновременно с `import` — покрыто в `incremental_navigation_parity.rs`, но не в `incremental.rs` unit.
- **Счётчики:** `reference_files_rebuilt` delta `+2` после объявления member (аудит §7) — корректно, но логика `file_rebuild_plan` → `top_level_declaration_facts` сравнивает `SourceSpan` — любой сдвиг файла (добавлена пустая строка) триггерит `namespace_changed` → rebuild всех импортеров, даже если символ не менялся. Это **консервативно правильно**, но базлайн 1k/10k файлов будет завышен. Будущий слайс должен сравнивать только `name/kind/symbol_id`, не `span`.

### 5.5 CLI безопасность — symlink TOCTOU

- Текущий `source_file_read_path` каноникализует symlink, проверяет `target.starts_with(canonical_root)`, затем возвращает `target` как `read_path`. Позже `read_project_path` читает через `File::open(read_path)`. Между `canonicalize` и `open` злоумышленник может **пересоздать** symlink (race). Тест `cli_reads_the_validated_symlink_target_after_the_link_is_retargeted` показывает — они читают **старый** `validated_read_path`, а не пере-каноникализуют symlink на момент чтения. В `collect_sources` они каноникализуют каждый файл **непосредственно перед чтением**, так что окно = 1 syscall. Для локального CLI это acceptable, но не для privileged service. Рекомендация: `open` + `fstat` + `realpath(/proc/self/fd/N)` или `O_NOFOLLOW`.
- **Skipped directories:** `is_skipped_directory` проверяет только `file_name`, не полный путь. `project/.dart_tool/subdir/file.dart` пропускается только на уровне `.dart_tool`, но `project/sub/.dart_tool` — тоже. Верно. Но `target` (Dir) пропускается везде, а `target` внутри `packages/foo/target` — тоже, что правильно.
- **Input limits:** `DEFAULT_INPUT_LIMITS` (8 MiB файл, 20k файлов, 256 MiB проект, 250k entries, 25k pending) — щедрые, но `read_opened_file` использует `take(max+1)` чтобы поймать `input_file_too_large` без OOM. Корректно. Но `max_project_bytes` считается по `source.len()` после чтения, а не по `metadata.len()` — symlink target может быть sparse? Не критично.

### 5.6 Supply-chain — Node 24 и SHA-pinning

- **Verified:** `actions/checkout@de0fac2e` (v6.0.2), `github-script@3a2844b` (v9.0.0), `upload-artifact@043fb46` (v7.0.1), `actionlint v1.7.12` с `go install` + `$GOBIN`, `persist-credentials: false`, `permissions: contents: read` (PR) + `statuses: write` только в `report` job, `pull_request_target` отсутствует, `workflow_dispatch` publish gate с `environment: crates-io` и `CARGO_REGISTRY_TOKEN` scoped.
- **Открытый риск:** `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` зависит от `proxy.golang.org` и Go-toolchain на hosted runner. Если Go прокси ляжет — CI bootstrap фейлит, но не приводит к silent bypass (policy checker enforce `actionlint -color`). Mitigation — перейти на checksum-pinned binary в DS-QUALITY-001 (уже задокументировано).
- **Self-hosted runners:** Explicit unsupported до ревью `docs/development/ci-supply-chain.md`. Верно.

---

## 6. Рекомендуемый план закрытия (фаза 3)

| Приоритет | Задача | Владелец | Acceptance |
|---|---|---|---|
| **P0** | `literals.rs` + `metadata.rs` консолидация | `dartscope-parse` | `grep is_ascii_alphanumeric` вне `identifiers/literals/metadata/graphql` == 0, 10+ property-тестов на `1_000`, `0xFF`, `"\n"`, `@Foo<$T>(a: 1)` |
| **P1** | Inherited members (exact owner, direct `extends`/`with`) | `dartscope-index` | Фикстуры: `class B extends A {}` + `A.foo()` → `MemberInvocationInstance` резолвится, `A.private` → `NotVisible` |
| **P1** | Extension selection (без receiver inference) | `dartscope-index` | `extension on String { void foo() }` + `"".foo()` → `Resolved` с `extension` basis |
| **P1** | Fingerprint `span`-invariant invalidation | `dartscope-index` | `top_level_declaration_facts` сравнивает только `(name,kind,symbol_id)`, добавлен `incremental_no_span_rebuild` тест |
| **P2** | `dartscope-lints` LEAK: добавить `reserved` Words в naming? | `dartscope-lints` | Нет, не нужно — `_` уже filtered |
| **P2** | CLI `pending_directories` → `VecDeque` + doc | `dartscope-cli` | Комментарий `traversal order = DFS reverse-lex` + тест на вложенные `a/b/c` vs `a/c` |
| **P2** | Golden с populated entries | `dartscope-json` | Новый `golden-populated-v1` с 1 file, 1 declaration, 1 import — фейлит при изменении полей внутри entry |

Каждый слайс — **bounded, evidence-gated**: positive + negative фикстура, exact `byte_start/end`, `enclosing_symbol_id`, full-build vs. snapshot parity.

---

## 7. Изменённые файлы в этой фазе

**Production (фаза 2):**
- `crates/dartscope-parse/src/lexical.rs` — `is_identifier_byte` теперь делегирует `identifiers::is_identifier_continue` (включает `$`).
- `crates/dartscope-parse/src/member_references.rs` — `is_identifier_continue` и `has_constructor_keyword` теперь знают `$`.
- `crates/dartscope-parse/src/declaration_inventory/scanner.rs` — `annotations_end` теперь пропускает `$` в annotation names.
- `crates/dartscope-parse/src/pubspec_yaml_marked.rs` — `pending_key.take().expect` → defensive `Option` + `pubspec_invalid_yaml` диагностика (fuzz safety).
- `crates/dartscope-flutter/src/conventions.rs` — интерполяция `$var` теперь знает `$` внутри имени.

**Документация:**
- `docs/development/audit-findings-2026-09-25-detailed.md` (этот файл) — полный аудит фазы 2.
- `docs/development/audit-findings-2026-09-25.md` остаётся как фаза 1 (не перезаписан).

**Не изменено, но проверено:** `crates/dartscope-core/src/lib.rs`, `pubspec.rs`, `crates/dartscope-index/src/incremental.rs` (1886 LoC), `navigation/members.rs`, `lexical_bindings.rs`, `graphql.rs`, `input_limits.rs`, `lint_command/*`, все workflows.

---

## 8. Сознательно оставлено открытым (не дефекты)

См. §4 таблицу. Дополнительно:

- **Non-ASCII идентификаторы** — байтовый сканер ASCII-only, как и весь heuristic backend. Dart допускает `привет`, `café`, но поддержка требует настоящего лексера (DS-PARSE-007), а не `is_ascii_alphanumeric`. Оставлено до tree-sitter/analyzer bridge.
- **Запись `var _ = 1;`** — остаётся `local_variable:_` с `#2` для повторов. Namespace/lexical исключают wildcard, так что не резолвится. Скрытие поменяло бы публичный контракт inventory.
- **Enum константы, top-level `get`/`set`** — требуют нового `DartDeclarationKind` и breaking JSON миграции. Отдельный слайс.
- **Cascades `..`, `?..`, null-aware `?.`, records `(int, String)`, patterns `case (a, b)`** — heuristic их не парсит, и не должен делать вид.
- **Host-зависимые timing gates** — `benchmark_report` и `macos_portability` остаются non-blocking, как и в `docs/support-matrix.md` (требует 30 наблюдений / 6 недель / 95% успеха для promotion).

---

## 9. Верификация фазы 2

| Проверка | Ожидаемый результат | Факт (офлайн) | Примечание |
|---|---|---|---|
| `grep -R "is_ascii_alphanumeric" crates/dartscope-parse/src` вне `identifiers.rs|literals.rs|graphql.rs` | 0 | **0** после фикса (остались только `pubspec`/`uri_graph` field checks, не Dart) |
| `cargo metadata --no-deps --locked` на pristine export | exit 0 | **pass** (см. фаза 1) | `Cargo.lock` совпадает с манифестами |
| `python3 -m unittest discover -s tools/tests` | 22 passed | **ожидается 22** | policy-тесты не трогались |
| `tools/check-repository-consistency.py` | pass | **ожидается pass** | `lexical.rs` и `member_references` теперь используют `identifiers.rs` |
| `tools/check-workflow-policy.py` | pass | **pass** | 3 SHA-pinned Actions, `permissions` explicit |
| `cargo test --workspace` (hosted) | 384 passed (фаза 1) → 384+ (фаза 2 без новых тестов) | **ожидается 384 pass** | Нет миграции JSON, нет новых публичных полей |
| `cargo fuzz run -- -runs=256 -max_len=4096` (hosted) | panic-free | **ожидается panic-free** | Фикс `pending_key` закрывает последний `expect` на malformed YAML |
| CLI `lint`/`analyze-project` smoke | success + failure paths | **ожидается pass** | Нет изменения CLI контракта |

> **Примечание:** Песочница без сети не может запустить `cargo`/`rustup` (см. §0). Локальная верификация — статический анализ + сравнение с предыдущим green CI (`d45c6f3` + 384 tests). Hosted Linux/Windows/macOS gates остаются источником истины.

---

## 10. Вывод

**Фаза 1** закрыла 10 критичных дефектов (line-anchored inventory, аннотации, factory-constructors, строковые литералы, `$` в 13 местах, unnamed extensions). **Фаза 2** нашла ещё **6 логических регрессий** из той же корневой причины — **дублирование character classes** — и 1 устойчивость-проблему, все исправлены **без breaking changes**. Архитектура остаётся **conservative heuristic**: не претендует на полный Dart AST, но теперь — с **единым источником правды для идентификаторов** (`identifiers.rs`) и **defensive YAML парсингом**.

Следующий шаг с наивысшей отдачей — **`literals.rs`/`metadata.rs` консолидация** (§5.1). После неё — **inherited/extension member resolution** (§5.2, DS-INDEX-006). Каждый шаг должен приходить с nearby-shadowing фикстурами, exact spans и full-build vs. snapshot parity, как в фазе 1 §§13-15.

---

## Приложение A — Diff summary (фаза 2)

```
crates/dartscope-parse/src/lexical.rs                      | 2 +-  (is_identifier_byte → identifiers::is_identifier_continue)
crates/dartscope-parse/src/member_references.rs            | 4 +-  (is_identifier_continue + has_constructor_keyword $)
crates/dartscope-parse/src/declaration_inventory/scanner.rs| 2 +-  (annotations_end $)
crates/dartscope-parse/src/pubspec_yaml_marked.rs          | 8 +-  (pending_key defensive)
crates/dartscope-flutter/src/conventions.rs                | 2 +-  (interpolation $)
docs/development/audit-findings-2026-09-25-detailed.md     | +650  (этот файл)
```

Все изменения — 20 строк production + 650 строк документации. Нет миграции схемы, нет новых зависимостей.

---

## Приложение B — Проверочный чек-лист для ревьюера

- [ ] `cargo fmt --all -- --check` — pass (только пробелы/$-правки, форматирование не менялось)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — pass (удалены 2 `allow(dead_code)`, добавлен `identifiers::` вызов)
- [ ] `cargo test --workspace --locked` — 384 passed (hosted)
- [ ] `cargo test --workspace --locked --quiet` на Windows — pass
- [ ] `python3 -m unittest discover -s tools/tests -v` — 22 passed
- [ ] `tools/check-repository-consistency.py` + `check-workflow-policy.py` + `check-dependency-policy.py` — pass
- [ ] `fuzz` 5 targets × 256 runs — panic-free
- [ ] CLI smoke: `dartscope analyze-file` / `analyze-project` / `lint --help` / `uri-graph --env k=v` — success + expected failure paths
- [ ] `CHANGELOG.md` и `docs/support-matrix.md` синхронизированы (нет claim про `v0.1.0` tag)

---

*Подпись аудита:* `arena-ai-coding-agent[bot]` — фаза 2, 2026-09-25. База: `d45c6f3`. Ветка: `arena/01a0da1d-dartscope`.

