---
id: doc://docs/development/audit-findings-2026-09-30.md
kind: development_note
language: ru
source_language: ru
status: active
---

# Инженерный аудит DartScope — 2026-09-30 (фаза 3: с реальным исполнением кода)

> **Ключевое отличие от аудитов 2026-09-25.** Оба прошлых аудита были статическими либо опирались на
> прогон, выполненный до последних изменений; ни один из них не собирал `main` после PR #159.
> Этот аудит **компилировал и запускал код** (Rust 1.95.0, GitHub Actions: macOS 15 arm64,
> Windows Server 2025, Ubuntu 24.04): собирал workspace, гонял тесты, запускал release-CLI на
> синтетических входах и проверял гипотезы Rust-тестами. Каждая находка помечена способом
> подтверждения (§1.2); то, что выполнить не удалось, перечислено в §1.3 и §15.2.

*Дата проверки:* 2026-09-30 (UTC).
*Коммит:* `5df9945` (merge PR #159), ветка аудита `arena/01a0f406-dartscope`.
*Язык документа:* русский (язык запроса).
*Статус исправлений на 2026-10-01:* большая часть находок исправлена в той же ветке — см. §16 (что сделано, что оставлено по решению, что отложено).

---

## 0. Краткий вердикт

**`main` сейчас не собирается, не проходит CI и не может быть выпущен.** Причина — не один дефект, а
несколько независимых, внесённых одним изменением (PR #159 «DS-LSP-001», коммит `eb7421f` и
связанный «аудит фазы 2»), которое **ни разу не компилировалось** перед слиянием:

| # | Что сломано | Почему это важно |
| --- | --- | --- |
| 1 | `dartscope-parse` **не компилируется** (3 ошибки в `pubspec_yaml_marked.rs`: лишние `\"`, `continue` вне цикла, `let … else` не расходится) | Без него не собираются CLI, индекс-тесты, Flutter, lints, LSP, umbrella — фактически вся библиотека |
| 2 | `Cargo.lock` не содержит `dartscope-lsp` | Любая команда с `--locked` (а она обязательна в AGENTS.md и во всём CI) падает **ещё до компиляции** |
| 3 | `dartscope-lsp` не компилируется (5 ошибок), 3 из его 11 unit-тестов красные после исправления компиляции, а по коду сервер **не совместим с протоколом** (capabilities сериализуются в `snake_case`, без `camelCase`) | «Реализованная» по плану функция LSP непригодна для клиентов (подтверждено протокольным прогоном по проводу: §3.7) |
| 4 | `cargo fmt --check` и `clippy -D warnings` красные | Блокируют `Quality gates` и `Release` |
| 5 | 2 теста красные даже после исправления компиляции; один из них вскрывает **регрессию корректности инкрементального индекса** | `per_file_caches_rebuild_only_relevant_sources` |
| 6 | Число крейтов «9» зашито в CI, benchmark-скрипте и в тесте, который закрепляет устаревшее значение | После добавления 10-го крейта macOS- и benchmark-джобы падают независимо от всего остального |

Помимо блокеров, аудит нашёл **функциональные дефекты** (BOM скрывает первую декларацию, функции и методы с `<T>` не
попадают в инвентарь, ложные Flutter-виджеты из `extension … on Widget`, «выдуманные» цели навигации через чужие extension,
устаревшие span'ы в инкрементальных снимках, аварийный выход CLI при не-UTF-8 аргументах и при
закрытом stdout, отказ всего прогона из-за одного symlink на каталог — типично для Flutter-проектов,
**квадратичная зависимость времени анализа от размера файла**: 1,45 МБ — 108–140 с, а `flutter/samples` из 484 файлов
анализируется 223 с на Linux (два сгенерированных файла FFI/JNI-привязок, 2,7 и 1,1 МБ, дают почти всё время) и не
укладывается в 240 с на macOS; `lint` на нём же завершается кодом 6 без единого finding из-за
пустой секции `flutter:` в одном `pubspec.yaml`, и т. д.) и **пробелы реализации**. Все они приведены ниже с доказательствами и приоритетами.

Прежние аудиты (2026-09-25) содержат утверждения, которые не подтверждаются (см. §12), в том числе
«исправление», которое и сломало сборку.

---

## 1. Методология и границы

### 1.1 Что делалось

1. **Статическое чтение** кода и документации: все 10 крейтов, CI, `tools/`, `fuzz/`, `docs/`.
2. **Удалённая сборка и тесты** на GitHub Actions (Rust **1.95.0**, как в `rust-toolchain.toml`).
   Локально в песочнице Rust недоступен (нет доступа к `static.rust-lang.org`/`crates.io`), поэтому
   на ветке аудита временно существовал «зонд» — набор скриптов и workflow (`audit-probe/`,
   `.github/workflows/audit-probe*.yml`), который собирал код, гонял тесты, запускал CLI на
   синтетических входах, запускал Rust-пробы и возвращал результаты через аннотации check-run.
   **Зонд удалён из итогового изменения**; продуктовый код не менялся.
3. **Дифференциальная проверка** парсера против официального Dart-парсера (`package:analyzer`) на реальных
   репозиториях была подготовлена и запущена, но **не выполнена** (сбой установки зависимостей, §15.2).
4. Для сборки в зонде применялся **только внутрираннерный нейтральный патч**, убирающий
   синтаксическую ошибку, чтобы добраться до следующих слоёв; «как закоммичено» проверялось
   отдельным проходом.

### 1.2 Пометки достоверности

| Пометка | Значение |
| --- | --- |
| **[CI]** | Воспроизведено исполнением на GitHub Actions (указан контекст) |
| **[стат.]** | Следует из чтения кода; на исполнении не проверялось |
| **[док.]** | Расхождение документации и кода, установленное сравнением |

### 1.3 Чего сделать не удалось (честно)

* **Локально Rust недоступен** (нет доступа к `static.rust-lang.org`/`crates.io`); всё исполнение — на GitHub Actions.
  Ubuntu-раннеры в день проверки выдавались с большой задержкой (десятки минут), поэтому основной объём — macOS и Windows;
  Linux подтверждён на тестах, CLI-батарее, масштабировании, корпусе и мутационном fuzz (результаты совпадают с macOS).
* **Токен GitHub в песочнице на время истекал** (≈21:35–22:05 UTC, `Bad credentials`): в этот промежуток были недоступны
  `git push` и `gh`, а результаты идущих прогонов я читал через публичное API. После восстановления доступа выполнены
  заключительные раунды (протокол LSP по проводу, матрица форм, `package_config`/`uri-graph`, замер по файлам
  `flutter/samples`, проверка патчей, профиль); результаты — в соответствующих разделах.
* **Не выполнена** и остаётся **гипотезой** дифференциальная проверка против `package:analyzer` (причина — сбой установки
  зависимостей, подробности в §15.2).
* Fuzz-цели (`cargo fuzz`, nightly) не запускались.
* Живая загрузка SARIF в GitHub Code Scanning не проверялась (§7).

---

## 2. Сборка, CI и релизные гейты (P0)

### 2.1 Состояние CI на `main`

Источник: GitHub Actions, прогоны на коммите `5df9945` (merge PR #159) и плановый прогон от 2026-09-28
(`CI` 36185507113, `Release` 36185507160, `CI (schedule)` 36431529044; результаты шагов получены через
REST API — тексты логов недоступны, но имя упавшего шага однозначно).

| Джоб CI | Результат | Упавший шаг | Причина (подтверждена в §2.2–2.6) |
| --- | --- | --- | --- |
| Workflow policy | успех | — | — |
| Dependency security and hygiene | успех | — | `cargo audit`/`machete` чисты |
| Quality gates | **провал** | `Check formatting` | `cargo fmt --check` (§2.4) |
| Bounded fuzz corpus | **провал** | `Check fuzz bridge and formatting` | `--locked` + компиляция (§2.2, §2.3) |
| Benchmark regression report | **провал** | `Compare benchmark workloads` | компиляция + жёсткое «9 архивов» (§2.5) |
| Tests (ubuntu / windows) | **провал** | `Run workspace tests` | `--locked` + компиляция |
| macOS 15 arm64 portability | **провал** | `Check and test workspace` | то же; далее упал бы `archive_count = 9` |
| Edition 2024 × 6 (Linux/Windows) | **провал** | `Run edition check` | для `umbrella-minimal` это **только** устаревший `Cargo.lock` (проверено: без `--locked` команда проходит) |
| Release / Test and package | **провал** за 32 с | `Verify formatting` | `cargo fmt --check` |

Все PR-проверки PR #159 (`CI` 36185339768, `Release` 36185339750) были **красными до слияния**; PR был
слит вопреки этому. Плановый прогон 2026-09-28 показал «6 ч 0 мин 29 с» — это не зависание кода:
Windows- и macOS-джобы стартовали сразу и упали за ~30 с, а Ubuntu-джобы ждали раннер: первый из них
(`Workflow policy`) стартовал через 1 ч 47 мин после создания прогона (13:50:44 → 15:38:07 UTC), остальные — ещё 1 ч 37 мин –
1 ч 53 мин (созданы 15:38:29, старт 17:15–17:32); прогон завершился целиком в 19:51:12. Длительность — очередь платформы
GitHub (в этот же день мои Ubuntu-джобы ждали десятки минут), а не поведение кода. Это, впрочем, означает, что **красный статус на
Linux виден лишь через часы**; полезно держать быстрый «compile + fmt» гейт на Windows/macOS или задать таймаут очереди.

### 2.2 Ошибки компиляции `dartscope-parse` **[CI]**

`cargo check --workspace` (macOS arm64, Rust 1.95.0, коммит как есть):

```text
crates/dartscope-parse/src/pubspec_yaml_marked.rs:245:33: error: unknown start of token: \
crates/dartscope-parse/src/pubspec_yaml_marked.rs:419:41: error: character literal may only contain one codepoint
crates/dartscope-parse/src/pubspec_yaml_marked.rs:419:53: error: unknown start of token: \
crates/dartscope-parse/src/pubspec_yaml_marked.rs:419:55: error[E0762]: unterminated character literal
error: could not compile `dartscope-parse` (lib) due to 4 previous errors
```

Строки 245–246 содержат литеральные `\"pubspec_invalid_yaml\"` (обратный слеш вне строкового литерала);
остальные ошибки — каскад. После механического удаления слешей компилятор сообщает ещё две ошибки
в **том же** блоке:

```text
pubspec_yaml_marked.rs:249:29: error[E0268]: `continue` outside of a loop
pubspec_yaml_marked.rs:243:70: error[E0308]: `else` clause of `let...else` does not diverge
```

**Анализ причины.** Блок — «исправление» из аудита фазы 2 (CHANGELOG: «`pubspec_yaml_marked.rs`
no longer panics on malformed mappings…»). Заменён `pending_key.take().expect("mapping key must exist")`
в ветке `else` конструкции `if pending_key.is_none() { … } else { … }`. Но в ветке `else`
`pending_key.is_some()` гарантировано самим условием, то есть `expect` **не мог сработать** —
паники не было, а «исправление» добавило недостижимый (и невалидный) код. Запись в CHANGELOG о том,
что это «сохраняет fuzz-корпус без паник», не соответствует действительности.

**Следствие:** не собираются `dartscope-parse` и всё, что от него зависит (`dartscope-cli`,
`dartscope-flutter`/`-lints`/`-index` в тестах, `dartscope-lsp`, umbrella с фичей `parse`).

### 2.3 `Cargo.lock` рассинхронизирован с манифестами **[CI]**

`cargo update --workspace` на чистом коммите: `Adding dartscope-lsp v0.1.0`. Точная разница:

```diff
@@ dartscope (dependencies)
  "dartscope-lints",
+ "dartscope-lsp",
  "dartscope-parse",
@@
+[[package]]
+name = "dartscope-lsp"
+version = "0.1.0"
+dependencies = [
+ "dartscope-core", "dartscope-index", "dartscope-parse", "serde", "serde_json", "thiserror",
+]
```

Любой `cargo … --locked` (обязательный по AGENTS.md и использованный во всех джобах) падает с
«lock file needs to be updated». Важно: `check-repository-consistency.py` и `check-release-packages.py`
вызывают `cargo metadata --locked --no-deps` — этот вариант **не проверяет актуальность lock-файла**
(проверено: проходит на устаревшем lock), поэтому политики репозитория дефект не ловят. Рекомендация —
добавить в гейт `cargo metadata --locked` (без `--no-deps`) или `cargo check --locked`.

### 2.4 `cargo fmt --check` и Clippy **[CI]**

* `cargo fmt --all -- --check` падает минимум в: `dartscope-cli/tests/versioned_json_contracts.rs`,
  `dartscope-index/src/incremental.rs:1042`, `dartscope-index/src/navigation/members.rs:597,687`,
  `dartscope-lsp/src/bin/dartscope-lsp.rs` (несколько блоков) и др.; кроме того, в
  `dartscope-parse/src/lib.rs` нарушен алфавитный порядок `mod` (`literals; metadata; lexical…`).
* `cargo clippy --workspace --all-targets -- -D warnings` (после нейтрального исправления компиляции):
  * `lexical.rs:9` — `unused import: StringLiteralRange`;
  * `lexical.rs:112` — `function is_identifier_byte is never used` (мёртвый код; аудит-фикс «делегировать
    в `identifiers`» оставил функцию без вызовов);
  * `members.rs:249` — `this if statement can be collapsed` (`clippy::collapsible_if`).
* В `literals.rs` добавлено 4 функции с `#[allow(dead_code)]` (`is_digit`, `is_hex_digit`,
  `is_numeric_continue`, `numeric_literal_end`) — «централизованные предикаты» без единого вызова; это
  противоречит `rust-code-standards.md` (не прятать предупреждения) и фазе-1 утверждению «`allow(dead_code)` нет».

### 2.5 Жёстко зашитое число крейтов **[CI] + [стат.]**

После добавления `dartscope-lsp` крейтов стало 10 (`cargo package` создаёт 10 архивов, проверено), но
число «9» осталось в трёх местах, причём одно — в **тесте политики**, закрепляющем устаревшее значение:

| Файл | Строка |
| --- | --- |
| `.github/workflows/ci.yml` (macOS job) | `test "$archive_count" = "9"` |
| `tools/report_benchmark_regressions.py:192` | `if len(archives) != 9: raise BenchmarkError(…)` |
| `tools/tests/test_macos_portability_policy.py:26` | `assertIn('test "$archive_count" = "9"', body)` |

Плюс текстовый дрейф: README («All nine crates»), CHANGELOG («Nine publishable Rust crates», затем
«New optional crate `dartscope-lsp`»), `benchmark-regressions.md`, `dartscope-library-plan.md`
(«nine crates/archives» ×6). Рекомендация — вычислять ожидаемое число из `tools/release-crates.txt`.

### 2.6 Тесты после нейтрального исправления компиляции **[CI]**

Windows Server 2025 и macOS 15 arm64, `cargo test --workspace --exclude dartscope-lsp --no-fail-fast`:
**388 прошли / 2 упали / 1 ignored** (macOS arm64 и Ubuntu 24.04 — результаты идентичны), **381/2/1** (Windows; unix-only symlink-тесты исключены).

| Тест | Причина |
| --- | --- |
| `dartscope-parse: literals::tests::dollar_in_identifier_does_not_break_raw_detection` | Тест утверждает `find_string_literal_start("foo$r'bar'", 0).is_none()`, но сканер (корректно) находит **обычную** строку `'bar'` с байта 5; неверна сама проверка. Тест не запускался до слияния. |
| `dartscope-index: tests::incremental::per_file_caches_rebuild_only_relevant_sources` | **Регрессия корректности**, см. §5.2: изменение «span-инвариантных fingerprint'ов» (PR #159) перестало инвалидировать зависимые файлы при изменении члена класса. |

Остальной тест-набор репозитория зелёный: 388 проходящих тестов (macOS) в десятках тестовых бинарей.

---

## 3. `dartscope-lsp` (новый крейт, 1 662 LoC, статус в плане — «implemented»)

План (`DS-LSP-001`) и CHANGELOG объявляют сервер реализованным: «initialize, didOpen/didChange/didClose,
definition, references, hover, documentSymbol, diagnostics» и «honest empty responses». Проверка показала,
что крейт **не собирается**, а после исправления компиляции **не совместим с LSP-клиентами по формату**
(подтверждено протокольным прогоном по проводу, §3.7).
Крейт также не описан в README и не имеет собственной страницы в `docs/development/`.

### 3.1 Компиляция **[CI]**

`cargo check -p dartscope-lsp` (macOS, Windows):

| Где | Ошибка |
| --- | --- |
| `server.rs:151, 202, 250` | `E0308`: `DartWorkspaceResolutionContext::from_snapshot` принимает `&DartWorkspaceSnapshot`, передан `Arc<DartWorkspaceSnapshot>` (`index.snapshot()`) |
| `server.rs:395` | `E0277`: `Diagnostic: Default` не выполнено (`..Default::default()` при отсутствии `derive(Default)`) |
| `bin/dartscope-lsp.rs:150` | `E0505`: `server.document_symbols(&params.text_document.uri, params)` — заём и перемещение `params` |
| `server.rs:11,16` | 5 неиспользуемых импортов (`byte_offset_to_lsp_position`, `TextDocumentContentChangeEvent`, `TextDocumentItem`, `VersionedTextDocumentIdentifier`, `WorkDoneProgressOptions`) |

Из-за этого красными были бы umbrella `--all-features` (проверка «Edition 2024 / umbrella-all-features»),
`cargo test --workspace` и `cargo package`-гейты, даже при исправленном `Cargo.lock`.

**После** минимального исправления компиляции (патч §14.1) запуск 11 unit-тестов крейта **[CI, macOS]**:
**8 прошли, 3 упали** — `coordinates::tests::handles_crlf`, `server::tests::diagnostics_published_for_unsupported_syntax`,
`server::tests::did_open_and_definition_round_trip`. Проходят: `initialize_returns_capabilities`, `incremental_change_applies_utf16`,
`rapid_file_replacement_keeps_index_consistent`, `hover_returns_kind_and_name`, `byte_offset_round_trips_lf`,
`handles_emoji_2_utf16_units`, `out_of_bounds_returns_none`, `source_span_round_trip`. Заявленное в плане «(tests for) hover and
`unsupported_concise_constructor` diagnostics» наполовину недостоверно: тест диагностики красный.

### 3.2 Формат протокола: сервер не распознаётся клиентами **[CI: G01 — ответ `initialize`; прочее — стат., семантика serde]**

В `types.rs` **нет `#[serde(rename_all = "camelCase")]`** у `ServerCapabilities`, `InitializeResult`,
`InitializeParams`, `TextDocumentSyncOptions`, `*Options`. Следствия (без запуска, но по семантике
serde детерминированы):

1. Ответ на `initialize` содержит `text_document_sync`, `definition_provider`, `references_provider`,
   `hover_provider`, `document_symbol_provider` и `server_info` вместо `textDocumentSync`,
   `definitionProvider`, `referencesProvider`, `hoverProvider`, `documentSymbolProvider`, `serverInfo`.
   Ни один клиент не увидит возможностей сервера → не будет слать `didOpen/didChange` (sync=none)
   и запросы `definition/hover/references/documentSymbol`.
2. Поля `rootUri`/`rootPath` из `initialize` **не читаются** (в структуре они называются
   `root_uri`/`root_path`; клиентские значения уходят в `#[serde(flatten)] extra`). Корень проекта всегда
   остаётся `"."`.
3. `SymbolTag` — `#[repr(u8)] enum` с обычным `derive(Serialize)` сериализуется как строка `"Deprecated"`,
   а не число `1`; `DefinitionOptions.work_done_progress_options` имеет неверную вложенность.

Юнит-тесты крейта (6 в `server.rs`, 5 в `coordinates.rs`) вызывают методы `DartLspServer` напрямую и
**ни один не проверяет JSON-ответ**, поэтому эти дефекты ими не ловятся.

### 3.3 Обработка JSON-RPC (`bin/dartscope-lsp.rs`) **[стат.; п. 1–6, 9 подтверждены по проводу: G06, G11, G07, G12, G14, G10, G03 — §3.7]**

| # | Дефект | Строки | Последствие |
| --- | --- | --- | --- |
| 1 | Запрос с невалидными `params` → `.ok()?` → **ответа нет вообще** | 119, 129, 139, 148 | Клиент ждёт ответ бесконечно (нарушение «каждый request получает response, -32602») |
| 2 | Невалидный JSON в теле молча отбрасывается (`Err(_) => continue`) | 50–53 | Нет `-32700 Parse error` |
| 3 | Неизвестный **запрос** получает `result: null`, а не `-32601 MethodNotFound` | 156–163 | Клиент считает функцию «поддержанной, но пустой» |
| 4 | Запросы до `initialize` не отвергаются; `shutdown` без `initialize` отвечает `null` | 87–90 | Нет `-32002 ServerNotInitialized` |
| 5 | `exit` всегда завершает процесс кодом 0 | 64–66, 68 | По спецификации при `exit` без `shutdown` код должен быть 1 |
| 6 | `Content-Length` разбирается в `usize` и идёт в `vec![0u8; len]` без лимита | 41–49 | `Content-Length: 99999999999` → попытка аллокации, аварийное завершение/DoS |
| 7 | `Content-Length: 0`/отсутствие заголовка → `continue` | 45–47 | Тихий пропуск сообщения |
| 8 | `serde_json::to_string(&response).unwrap()` | 55 | Паника в продуктовом пути (нужен комментарий-инвариант или обработка) |
| 9 | **`textDocument/publishDiagnostics` не отправляется нигде** | — | Метод `diagnostics()` существует, но недостижим из бинарника → диагностики в редакторе не появятся (CHANGELOG/план утверждают обратное) |

### 3.4 Синхронизация документов и координаты **[стат.; паника и порча документа подтверждены по проводу: G08, G05 — §3.7]**

* `server.rs:104–128` (`did_change`): при `range.start > range.end` вызов `String::replace_range(start..end, …)`
  **паникует** — внешний клиент может уронить сервер. Если позиция вне строки
  (`lsp_position_to_byte_offset → None`), **весь документ заменяется текстом изменения** (тихая порча
  документа), тогда как спецификация велит зажимать позицию до конца строки.
  Версия документа игнорируется (нет защиты от out-of-order).
* `coordinates.rs`: `lsp_position_to_byte_offset` возвращает `None` для `character` больше длины строки;
  спецификация требует «clamp to line length». Строки делятся только по `\n` (LSP/Dart считают `\r`
  самостоятельным разделителем). Тест `handles_crlf` нумерует байты неверно (в `"a\r\nb\r\nc"`
  смещение 3 — это `b`, то есть начало строки 1, а тест ожидает `(0,1)`) — **красный на CI** (`coordinates.rs:254`).

### 3.5 URI и пути **[стат.; подтверждено по проводу: G09 — §3.7]**

* Собственный тип `Url(String)`: `Url::parse` принимает любую строку с `://`, **нормализации нет**
  (регистр схемы, `%`-кодирование, dot-сегменты).
* `percent_decode` превращает каждый `%XX` в `char` напрямую (`byte as char`) — многобайтовый UTF-8
  (`%D0%BF` → «Ð¿») **декодируется в mojibake**. Любой путь с кириллицей, CJK и т. п. портится.
* `path_to_uri` строит `file://` + путь **без percent-кодирования**; затем `documents.get(&target_uri)`
  ищет документ по реконструированному URI — он не совпадает с исходным ключом (`file:///c%3A/…`
  у VS Code на Windows, любые пробелы/`%`/`#`/не-ASCII). При промахе код **молча подставляет текст
  запрашивающего документа** (`unwrap_or(content.as_str())`) и считает диапазон чужого файла по
  чужому тексту → неверные `Range` и «невозможные» позиции.
* `uri_to_path` отрезает ведущий `/` у абсолютных путей; корень проекта сервер не читает (§3.2),
  поэтому пути индекса — «полные без `/`», а не относительные к корню: сопоставление с `pubspec`/`lib/`
  построено на относительных путях и для LSP не сработает.

### 3.6 Модель «workspace» и производительность **[стат.; задержка измерена: G16 — §3.7]**

* Индексируются **только открытые буферы**; `did_close` **удаляет файл из индекса** (`rebuild_index`).
  `pubspec.yaml`/`package_config.json` не загружаются (`DartProjectInput::new(root, files, Vec::new())`),
  поэтому `package:`-импорты не резолвятся, переход в неоткрытый файл невозможен. Для «иде-сервера» это
  принципиальный пробел (нет `workspace/didChangeWatchedFiles`, нет сканирования корня; дизайн
  «без I/O» нигде не компенсирован клиентским протоколом `workspace/*`).
* `rebuild_index()` на **каждое** `didOpen/didChange/didClose` заново анализирует **все** открытые документы
  (`analyze_project_with_references`) и создаёт новый `DartWorkspaceIndex::from_reference_project`;
  инкрементальные `upsert_file_with_references`/`remove_file` **не используются**. Комментарий в
  `server.rs` («the incremental index itself is reused internally») неверен.
* Каждый `definition/references/hover` строит `DartWorkspaceResolutionContext::from_snapshot(...)`:
  это **клон всего проекта + повторное разрешение всех ссылок** на каждый запрос (контекст не кэшируется по
  `snapshot.generation()`), хотя документация контекста обещает переиспользование.
* `references` игнорирует `context.includeDeclaration`; `hover` печатает `Debug`-форму enum (`"Class Foo"`);
  `document_symbols` принимает неиспользуемый `_params`, отображает неизвестные виды в `Variable`/`Property`
  (локальные переменные функций становятся «детьми»-свойствами).
* Защита «children == все top-level» при `symbol_id == None`: фильтр
  `m.parent_symbol_id.as_deref() == decl.symbol_id.as_deref()` при `None` совпадёт со всеми top-level
  объявлениями (латентно: парсер сейчас всегда задаёт `symbol_id`).

### 3.7 Протокольный прогон по проводу **[CI]**

Раннер macOS 15 arm64, сервер `dartscope-lsp` (debug) из исправленного компиляцией дерева (патч §14.1), прогон
36783493869, job 110119249858. Зонд вёл себя как клиент: заголовки `Content-Length`, JSON-RPC по stdio.

| Проба | Наблюдение | Вывод |
| --- | --- | --- |
| G01 `initialize` | `result.capabilities` = `{definition_provider, document_symbol_provider, hover_provider, references_provider, text_document_sync: 2}`, рядом `server_info` — **snake_case** | клиент не найдёт `textDocumentSync`, `definitionProvider`…; сервер выглядит «пустым» (§3.2) |
| G03 `didOpen` | после открытия документа **ни одного уведомления** от сервера | `publishDiagnostics` не отправляется (§3.3, п. 9) |
| G04 `definition`/`hover`/`references` (один файл) | `definition` на `Foo()` и на локальной `a` — корректные диапазоны; `hover` → `{"contents": ["Class Foo"]}` (Debug-форма); `references` при `includeDeclaration: true` вернул **только использование**, без объявления | базовая навигация в пределах файла **работает** (позитив); `includeDeclaration` игнорируется |
| G04 `documentSymbol` | у дочерних символов `selectionRange` `(1,0)–(1,12)` при `range` `(1,2)–(1,12)`; `(2,0)–(2,15)` при `(2,2)–(2,15)`: **`selectionRange` не лежит внутри `range`** | нарушение спецификации: `selectionRange` «Must be contained by the `range`» ([определение `DocumentSymbol`](https://github.com/microsoft/language-server-protocol/issues/1247)); клиенты, проверяющие условие (по моим сведениям — VS Code; здесь не проверялось), отвергают ответ. **Новая находка**: `selectionRange` должен покрывать имя, а не всю строку с отступом |
| G05 позиция за концом строки | `character: 500` → `result: []` | спецификация велит зажимать позицию до конца строки |
| G06 невалидные `params` | `definition` с `{"oops": true}` → **ответа нет** (ожидание 3 с) | клиент «зависает»; нужен `-32602 InvalidParams` |
| G07 неизвестный запрос | `workspace/symbol` → `{"result": null}` | нужен `-32601 MethodNotFound` |
| G08 `didChange` | `start > end` → **процесс упал: exit 101**, `slice index starts at 6 but ends at 2`; `end` за длиной строки → документ **заменён целиком** (остался `Q`, строка `class Bar {}` потеряна); `end` за последней строкой — корректно | паника от клиентского ввода; тихая порча документа |
| G09 кросс-файловый `definition` | `file:///tmp/proj/lib` — верно; `my%20proj` → `file:///tmp/my proj/…` (**без кодирования**); кириллица → **mojibake** `file:///tmp/Ð¿Ñ…/lib/b.dart`; `file:///c%3A/…` → `file:///c:/…`; во всех трёх случаях `range` неверный — `(1,9)–(1,24)` вместо `(1,0)–(1,15)`: он посчитан по тексту **запрашивающего** документа | переход не находит файл/ставит курсор не туда для путей с пробелом, не-ASCII и для VS Code на Windows |
| G10 `Content-Length: 99999999999` | процесс **жив и молчит**, ждёт тело; ошибки нет | нет предела размера сообщения (на Linux/Windows поведение не проверялось) |
| G11 невалидный JSON | следующий валидный запрос обработан («recovered»), но на плохое сообщение **нет ответа `-32700`** | |
| G12 запрос до `initialize` | `definition` → `{"result": []}` | нужен `-32002 ServerNotInitialized` |
| G13/G14 завершение | `shutdown`+`exit` → код 0 (верно); `exit` без `shutdown` → код **0** | по спецификации — 1 |
| G15 CRLF + эмодзи | `definition` на `B()` → `(2,0)–(2,10)` | **корректно** (позитив) |
| G16 задержка | 150 открытых документов: открытие всех — **6,1 с**; 10 правок всего текста + запрос — **1,08 с (108 мс на правку)**; один `definition` — 3 мс | полная пересборка индекса на каждое изменение (§3.6); рост ∝ числу открытых документов (на 1 000 файлов по экстраполяции ≈ 0,7 с на правку) |

Проба `G02` (чтение `rootUri`) в скрипте — не измерение, а комментарий; вывод §3.2, п. 2 остаётся **[стат.]**.
Итог: в пределах одного файла базовые запросы отвечают корректно (кроме `includeDeclaration` и `selectionRange`), но
**как LSP-сервер он не пригоден**: клиент не увидит его
возможностей (G01), диагностик (G03), получит зависание на ошибочных запросах (G06), аварийное завершение от правки
(G08) и неверные цели для путей вне чистого ASCII (G09).

---

## 4. Парсер `dartscope-parse` (15 174 LoC, эвристический бэкенд)

Все пункты с пометкой **[CI]** получены реальным запуском release-бинарника CLI (macOS arm64) на
синтетических входах и через Rust-интеграционные тесты на macOS/Windows; там, где это важно, указан
идентификатор кейса (`A..`, `P..`).

### 4.1 UTF-8 BOM скрывает первую декларацию и первую директиву **[CI]** (P1)

| Кейс | Вход | Результат | Ожидалось |
| --- | --- | --- | --- |
| A02 | `\u{FEFF}import 'package:a/a.dart';\nclass A {}` | `imports = []`, `class A` найден | импорт найден |
| A03 / P1 | `\u{FEFF}class First {}\nclass Second {}` | найден только `Second` | `First` и `Second` |

BOM (U+FEFF) не является `char::is_whitespace`, поэтому `trim()`/`first_code_byte` оставляют его перед
ключевым словом, и строка 1 перестаёт быть «началом объявления». BOM — обычное явление для файлов из
Windows-редакторов (в т. ч. первая строка `library`/`part of`/`import`): теряется первая директива, при
`part of` на первой строке ломается связывание part-файлов. **Тот же дефект в `pubspec.yaml`** (кейс C08, Linux): файл `\u{FEFF}name: app\r\nenvironment: …` даёт
`pubspec_missing_name` (команда `pubspec`; `pubspec-config` диагностики не выдаёт) — имя пакета теряется, а значит
не строится `package_roots`, и `package:`-импорты пакета остаются неразрешёнными.
*Рекомендация:* снимать BOM на входе (`DartFileInput::new`/`mask_non_code`) с сохранением смещений.

### 4.2 Пробелы инвентаря деклараций **[CI]** (P1)

| Кейс | Вход | Результат |
| --- | --- | --- |
| A3 (матрица форм, 34 формы) | `T f<T>(T a) => a;` и `Map<String, List<int>> f<U>(U a) => {};` на верхнем уровне; то же для метода класса (`T m<T>(T a)`, `Map<…> m<U>(U a)`) | **функция/метод с собственным списком type-параметров `<…>` не инвентаризируется** (нет `function:f` / `method:m`); при этом `List<int> f() => [];` находится — причина изолирована: именно `<T>` после имени. Затрагивает любые generic-функции (`T read<T>()`, `Future<T?> showX<T>()`, `Iterable<R> map<R>(…)`) → нет перехода к определению, ссылки и lint-правила их не видят |
| A3 | `void Function(int) f() => (i) {};` (возвращаемый тип — функциональный), так же метод | реальная функция/метод **теряется**, вместо неё создаётся **ложная декларация с именем `Function`** (`function:Function`, `method:Function`) |
| A3 / A16 | `(int, int) f() => (1, 2);` (record-возврат), так же метод | не инвентаризируется — документированное ограничение («records remain limited») |
| A12 / P2 | `enum Color { red, green, blue }`, `enum E2 { a(1), b(2); … }` | **константы enum не инвентаризируются** (только `enum:Color`, `enum:E2` и члены после `;`) |
| A13 / P2 | `int get total => 1; set total(int v) {}` на верхнем уровне | **top-level getter/setter не инвентаризируются** (функция, переменные — да) |
| A18 | `class G<T extends Object> { … Map<String, List<int>> m<U>(U a) => {}; }` | метод `m` отсутствует (остальные 6 членов найдены) — частный случай первой строки |

Из 34 форм матрицы **26 найдены корректно** (Future/async, nullable, именованные/дефолтные/аннотированные параметры,
`async*`/`sync*`, `external`, статический и блочный getter, setter, `operator ==`/`[]`, abstract, `@override`, factory,
const-конструктор с именованными параметрами, список инициализации, `late final`, несколько деклараторов в одной строке,
вложенные generic `>>>`, `covariant`, однострочное тело класса) и **8 пропущены** (4 на верхнем уровне и 4 в классе: по
две формы с `<T>` у функции, record-возврат, функциональный возвращаемый тип).

Record-возврат, enum-константы и top-level accessors — ограничения эвристического бэкенда, названные в README частично
(«records … remain limited»). **Generic-функции/методы и функциональный тип результата — недокументированные пропуски
обычных конструкций**; при статусе `DS-PARSE-006` = `verified` стоит добавить положительные фикстуры (в приложенный патч
регрессионных тестов включены `generic_functions_and_methods_are_inventoried` и
`function_type_return_does_not_create_a_bogus_declaration`) либо явно перечислить это в README как неподдерживаемое.

### 4.3 Лексическое маскирование и интерполяция строк **[CI]** (P2)

* Кейс A07: `final t = '${x.replaceAll("'", '')}';` (валидный Dart) → ложная диагностика
  **`unterminated_string`** и диагностический span, тянущийся до EOF. Маскер не понимает
  `${ … }`: кавычка внутри вложенной строки внутри интерполяции рвёт разбор.
* Кейс A06 (`'a ${b['c']} d'`) — сохраняется чётность кавычек, декларации после строки находятся
  (P3 прошёл). Следствие остаётся: внутри интерполяции маскируется **код** (ссылки внутри `${…}` не
  видны), а при нечётном числе кавычек во вложенной строке искажается остаток строки.
* **Реальный код (riverpod, 1 295 файлов, прогон `analyze-project`)**: **5 ложных `unterminated_string`** на валидных
  файлах (дублируются в проектном и файловом списках → 10 записей), например
  `packages/riverpod_generator/lib/src/riverpod_generator.dart` (диагностика начинается в байте 8267 у
  `'(${buildParamInvocation…` и тянется до EOF, строка 408) и
  `packages/riverpod_devtool/lib/src/state_inspector/inspector.dart` (с байта 12711 до EOF, строка 771); в `bloc`
  (616 файлов) — 0. Каждая такая диагностика протянута до конца файла (конвенция конца — §4.4), то есть помечает весь остаток файла. Код генераторов/шаблонов со вложенными строками в
  `${…}` — именно тот случай, где инструмент анализа должен быть тише.
* Поведение при незавершённых строках/комментариях корректно (диагностики
  `unterminated_string`/`unterminated_block_comment`, последующие декларации находятся — A10, A11).

### 4.4 Что проверено и работает **[CI]**

Из видимой части батареи (первые 21 кейс; остальное было обрезано лимитом аннотации): CRLF (A04); lone-`\r` как
разделитель (A05: декларации находятся, но номера строк считаются только по `\n`, тогда как Dart и LSP считают `\r`
разделителем); не-ASCII в комментариях/строках перед декларациями (A09); «ключевые слова» внутри строк и комментариев не
порождают деклараций (A08); Dart 3 модификаторы классов `sealed/base/interface/final/mixin class`, `abstract interface/base class`
(A14); extension types, typedef (в т. ч. `typedef R = (int, String)`), неименованные extension (A15); аннотации всех форм
(A17); factory/redirecting/named-конструкторы, `external`, `late final` (A18); Flutter-виджеты `StatelessWidget/StatefulWidget`
(A19); `library/import … show/hide/export/part` (A20). Проверка инвариантов span'ов на всех выводах (границы char, строка/столбец):
единственные «несовпадения» — конвенция EOF (позиция после завершающего `\n` отдаётся как конец последней строки, а не `(line+1, 1)`);
считаю это соглашением, а не дефектом.

### 4.5 Семантика `extends`/`mixes_in` изменена без смены схемы **[CI]** (P1)

PR #159 начал записывать в `DartDeclaration.extends` тип `on` у `extension`, а в `mixes_in` —
`on`-ограничения у `mixin`. Поля — часть **v1-контракта** (JSON и Rust), их смысл изменился, схема не
версионировалась, golden-фикстуры (`*-populated-v1.json`) это не ловят. Последствия подтверждены:

* `mixin M on A {}` → `mixes_in = ["A"]` (тест P6 падает): `on` — ограничение суперкласса, а не подмешанный тип.
* `extension X on String {}` → `extends = "String"`: потребители, трактующие `extends` как суперкласс, получают
  ложные данные (см. §6 — Flutter-виджеты).

### 4.6 Прочее

* **[CI]** `lexical.rs` — мёртвая функция и неиспользуемый реэкспорт (clippy), 4 `#[allow(dead_code)]` в `literals.rs`.
* **[стат.]** `annotations_end` (`metadata.rs`) по-прежнему использует собственный класс символов
  (`is_ascii_alphanumeric() || _ $ .`), а не `identifiers` — вопреки заявлению CHANGELOG о «единственном сканере».
* **[CI]** Квадратичная зависимость времени анализа от размера файла (1,45 МБ → 140 с; реальный `flutter/samples`
  не укладывается в 240 с) — §10 (вероятные причины — `span_for_byte_range`, per-owner проходы).
* **[стат.]** Шесть мест используют `expect("identifier token")` в продуктовых сканерах
  (`typed.rs:207,272`, `typed_positions.rs:382,614`, `lexical_bindings.rs:489`, `lexical_regions/scan.rs:131`):
  инвариант не документирован комментарием (требование `rust-code-standards.md`), а fuzz-целей на эти
  стадии нет (§11).

---

## 5. Индекс и навигация `dartscope-index` (7 678 LoC)

### 5.1 Навигация «выдумывает» цели через чужие extension **[CI]** (P1)

PR #159 добавил `refine_extension_member` (`navigation/members.rs`): если владелец не найден/не содержит
член (`Missing`), берётся **любое** extension проекта с членом того же имени. Фильтр по типу `on`
закомментирован («future on-type-aware filter»), а вместо проверки импорта подставляется
`basis = DirectImport` («exact import-graph check is deferred»). Тесты на Rust (macOS/Windows):

| Кейс | Вход | Результат | Должно быть |
| --- | --- | --- | --- |
| S1 | `class Foo { void run() { this.zap(); } }` + `extension StringX on String { int zap() => 1; }` | `Resolved → StringX.zap (SameFile)` | не `Resolved` (extension на `String` к `Foo` неприменимо) |
| S3 | `class Bar { void go() { this.extra(); } }` и `extension FooX on Foo { void extra() {} }` в **неимпортируемой** библиотеке | `Resolved → ext.dart:extra, basis=DirectImport` | не `Resolved`; «DirectImport» — недостоверное доказательство |
| S5 | член класса и одноимённый extension-член | `Resolved`, 1 цель — член класса | верно (приоритет класса соблюдён) |

Это прямое нарушение принципов проекта (`DS-INDEX-006`, «Required work» п. 4: «retain missing, ambiguous,
non-visible … rather than guessing»; п. 13 того же раздела: «no member fact is fabricated from … extension
lookup»; README: «Do not pretend heuristic findings are complete»). Практически: «Go to definition» /
«Find references» уводят на чужие декларации, а `find_references` для extension-члена приписывает ему
несвязанные вызовы.

### 5.2 Инкрементальный снимок расходится со stateless-анализом **[CI]** (P1)

PR #159 убрал `&declaration.span` из `top_level_declaration_facts` («span-инвариантные fingerprint'ы»,
CHANGELOG). Но кэшированные резолюции зависимых файлов (`DartIdentifierReferenceResolution.candidates[*]
.declaration_span`) **содержат спаны** целевых деклараций. Проверки (macOS):

| Кейс | Правка `lib/b.dart` | `affected_paths` | Снимок равен stateless-анализу? |
| --- | --- | --- | --- |
| I1 | `class B {}` → `\n\nclass B {}` (только форматирование) | `["lib/b.dart"]` | **нет** (устаревшие спаны в `lib/a.dart`) |
| I2 | `class B {}` → `class B { int value = 1; }` | `["lib/b.dart"]` | **нет** |

Тот же дефект ловит тест репозитория `per_file_caches_rebuild_only_relevant_sources`
(`src/tests/incremental.rs:263`: ожидается `["lib/a.dart","lib/b.dart"]`) — он **красный после PR #159**.
Контракт «snapshot ≡ stateless» (`incremental-index.md`, `assert_snapshot_matches`) нарушен. Минимальное
исправление — вернуть спаны в факты (проверено в §14) либо явно инвалидировать зависимых при смене спана цели.

### 5.3 Наследование — только один уровень **[CI]** (P2, пробел)

`refine_inherited_instance_member` проверяет прямые члены `extends`/`with`-предка, но не поднимается выше:
кейс S2 (`C extends B extends A`, `this.hello()` объявлен в `A`) → `Missing`. CHANGELOG формулирует «Direct
inherited…», но тогда результат `Missing` провоцирует ещё и fallback из §5.1. Цикл `A extends B, B extends A`
завершается (C4), длинная цепочка из 3 000 классов обработана без переполнения стека, но **2,86 с** на
один запрос (C3, Windows-раннер).

### 5.4 Что проверено и работает **[CI]**

Циклические реэкспорты (C1: 0,46 мс), цепочка реэкспортов из 1 500 файлов (C2: 53 мс, без переполнения
стека), 30 правок индекса из 400 файлов кольца импортов (C5: 66 мс), приоритет члена класса над extension (S5),
прямое наследование (S4). Все 52 unit-теста индекса, кроме §5.2, и весь набор интеграционных тестов навигации проходят.

### 5.5 Сложность и качество кода **[стат.]** (P2/P3)

* `MemberIndex::new` для **каждой** member-ссылки линейно ищет файл (`project.files.iter().find`) и затем
  декларацию в файле → O(ссылки × (файлы + декларации файла)); `refine_extension_member` перебирает **все** члены
  индекса на каждую неразрешённую ссылку и линейно ищет владельца (`find_declaration_by_symbol_id`);
  `external_namespace_uris` для каждой ссылки перебирает `uri_graph.references` внутри цикла по импортам.
  `DartWorkspaceResolutionContext::from_snapshot` **клонирует весь проект** (`project_reference_analysis()`) и
  заново разрешает все ссылки. Для больших проектов (и для LSP, где контекст строится на каждый запрос) —
  риск многосекундных пауз. Замеров навигации на больших проектах нет (измерен только парсинг, §10).
* Нарушения собственных стандартов репозитория (`rust-code-standards.md`): `incremental.rs` — **1 885 строк**
  (порог «разбить до новых фич» — 1 200), `DartWorkspaceIndex::rebuild` — **235** строк тела (порог 100);
  `members.rs` 911, `navigation.rs` 827, `typed_positions.rs` 846 строк (порог 800); `dartscope-core/src/lib.rs`
  (964 строки типов) и `dartscope-resolve/src/lib.rs` (635 строк реализации) не «тонкие»; 9 × `#[allow(clippy::too_many_arguments)]`
  вместо контекстных типов. Фаза-2 аудит назвала `incremental.rs` «верифицированным, но на грани» и цитировала
  порог «600 LoC», которого в стандарте нет (реальный — 1 200).

---

## 6. Flutter-конвенции `dartscope-flutter` (3 252 LoC)

### 6.1 Каждый `extension … on Widget` — «Flutter-виджет» **[CI]** (P1)

`derive_flutter_file_hints` создаёт `FlutterWidgetHint` для **любой** декларации с `extends`, совпавшим с
`Widget/StatelessWidget/StatefulWidget/InheritedWidget/State/ConsumerWidget`, **не проверяя `kind`**.
После PR #159 (§4.5) это включает extension'ы. Тест P-F1 (macOS):

```dart
import 'package:flutter/widgets.dart';
extension WidgetX on Widget { Widget padded() => this; }
extension on StatelessWidget {}
class Real extends StatelessWidget {}
```

Результат: `["WidgetX"<-Widget, ""<-StatelessWidget, "Real"<-StatelessWidget]` — **3 виджета вместо 1**,
включая неименованный extension с пустым `class_name`, `confidence = High`. Идиома
`extension X on Widget` — одна из самых частых во Flutter-коде. Тот же паттерн — `ecosystem.rs:359`
(`declaration.extends` → `base_class_pattern`) для Provider/Riverpod/BLoC-конвенций **[стат.]**.
Влияние: завышение `summary.flutter_widgets`, `flutter-inventory` (CLI), ложные findings.

### 6.2 Прочее **[стат.]**

* Виджет определяется только прямым `extends` из закрытого списка; транзитивные наследники
  (`class Screen extends BaseScreen`, где `BaseScreen extends StatelessWidget`) и `with`-миксины не учитываются.
* `imports_official_flutter(file)` пересчитывается для каждой invocation (O(invocations × imports)).
* README/plan называют конвенции «высокой уверенности», при этом `Widget`/`State` в списке баз делают
  «виджетом» и сам класс `State`-наследник; политика отсеивания — только по имени без проверки импорта.

---

## 7. Lint-движок `dartscope-lints` и `lint`-команда (1 242 LoC)

Поведение проверено на реальном бинарнике **[CI]** (macOS):

| # | Находка | Доказательство |
| --- | --- | --- |
| 1 | `dartscope lint <project>` **без `--config` запускает 0 правил** и возвращает exit 0 (`summary.enabled_rules = 0`) | D06 |
| 2 | Правило `naming_convention` **ложно срабатывает на сгенерированных именах**: `_$UserFromJson`, `_$UserToJson`, `jni$_init` помечены как «не lowerCamelCase» (в реальных `*.g.dart` таких функций сотни); исключить файлы по суффиксу `.g.dart` нельзя — только по **строковому префиксу** пути | D07; `naming.rs`, `config.rs` |
| 3 | `forbidden_import` и `layer_boundary` **не видят `export`** (`export 'package:forbidden/x.dart'`, `export '../data/repo.dart'`) и альтернативы **conditional-import** (`if (dart.library.io) 'package:forbidden/io.dart'`): найдены 3 из 6 нарушений (import-формы), обход через export/conditional — тихий | D08 матрица |
| 4 | Любой `DiagnosticSeverity::Error` в проекте (битый `pubspec.yaml` во вложенном `example/` или шаблоне, конфликт дубликатов) **прерывает весь lint** кодом 6 без единого finding | `lint_command.rs::malformed_project_message` |
| 5 | **Реальный случай: `lint flutter/samples` завершается exit 6 без единого finding после 224 с**, потому что в `ios_app_clip/pubspec.yaml` (последняя строка — **пустой `flutter:`**) диагностика `pubspec_invalid_flutter_configuration` («flutter configuration must be a mapping») имеет серьёзность Error. Файл **валиден**: инструмент Flutter прямо допускает значение секции `flutter` «object or null» ([`flutter_manifest.dart`](https://chromium.googlesource.com/external/github.com/flutter/flutter/+/master/packages/flutter_tools/lib/src/flutter_manifest.dart): `case 'flutter': if (kvp.value == null) { continue; }`) | **[CI]** Linux: `error: malformed project input at ios_app_clip/pubspec.yaml: pubspec_i…`; `analyze-project` тот же файл даёт 1 диагностику (Error) и exit 0; **[стат.]** `pubspec_yaml_marked_configuration.rs:107` — `flutter.value.mapping()` для null возвращает `None` → Error; **[док.]** исходник Flutter |

**Реальные репозитории [CI]** (все 5 правил, `entry_points = ["lib/main.dart"]`, прогон `runtime-long`): `bloc` — 53 предупреждения
`naming_convention` (29 имён функций, 19 имён файлов, 5 переменных верхнего уровня); `riverpod` — 297 (195 имён файлов,
66 имён функций, 36 `unresolved_part`). Я **не классифицировал** их на истинные и ложные (тексты сообщений усечены лимитом
аннотации), поэтому это данные о «шуме по умолчанию», а **не** доказанные ложные срабатывания; доказаны только `_$…`-имена (D07).
Для монорепозиториев без корневого `lib/main.dart` (оба случая) `orphan_file` молча не работает — см. ниже.

Дополнительно **[стат.]**: `source_prefix`/`ignored_path_prefixes`/`denied_target_prefixes` — простое `starts_with`
по строке без границы сегмента (`lib/data` совпадает с `lib/database/…`, `lib/gen` с `lib/generic/…`);
`uri_matches(Prefix)` — то же для URI (`package:flutter` совпадает с `package:flutter_bloc/…`); `orphan_file`
**молча отключается** при пустом списке `entry_points` и при опечатке в пути точки входа (`roots` пуст → `return`),
без диагностики.

### SARIF **[CI] + [стат.]**

Формат валиден по структуре (`$schema`, `version 2.1.0`, `columnKind: unicodeCodePoints`, `ruleIndex`,
`defaultConfiguration.level`). Замечания: `artifactLocation.uri` — **сырой путь без percent-кодирования**
(`lib/файл имя.dart`, `a#b.dart`, `a%b.dart` дают некорректный URI-reference; `#`/`?` читаются как fragment/query);
для результатов без span (имя файла, orphan) **нет `region`** — в минимальном примере GitHub Code Scanning `region`
присутствует, и документация предупреждает, что при отсутствии свойств данные «не будут отображаться корректно»
([GitHub Docs: SARIF support](https://docs.github.com/en/code-security/reference/code-scanning/sarif-files/sarif-support));
нет `originalUriBaseIds` при относительных `uri`. Загрузку в Code Scanning я **не проверял**.

---

## 8. CLI `dartscope-cli` (1 958 LoC)

Контракт (`cli-contract.md`): «CLI success writes only JSON to stdout; argument and filesystem errors write only
to stderr with stable exit codes». Проверка **[CI]** (release-бинарник; macOS arm64 и Ubuntu 24.04 дали идентичные результаты):

| # | Находка | Сценарий | Результат |
| --- | --- | --- | --- |
| 1 | **Паника на не-UTF-8 аргументе** (P2) | `dartscope analyze-file $'\xff.dart'` | exit **101**, `panicked at std/src/env.rs` — `env::args()` вместо `args_os()`; пути в Unix/Windows не обязаны быть Unicode |
| 2 | **Паника при закрытом stdout** (P3) | `dartscope analyze-file big.dart \| head -c16` | exit **101**, `println!` |
| 3 | **Один symlink на каталог роняет весь прогон** (P1/P2) | `ios/.symlinks/plugins/<dir> → вне корня` (создаётся `pod install`; аналогично `{linux,windows}/flutter/ephemeral/.plugin_symlinks/*`) | `analyze-project` и `lint`: exit **3** `input_symlink_rejected`; результата нет совсем |
| 4 | **Один не-UTF-8 `.dart` роняет весь прогон** (P2) | файл в CP1251 рядом с корректным | exit **3** `stream did not contain valid UTF-8` для всего проекта |
| 5 | **Каталоги `build`, `target`, `coverage`, `Pods`, `node_modules` пропускаются молча на любой глубине** (P2) | `lib/src/{build,target,…}/x.dart` | в выводе только `lib/main.dart`, **без диагностики о пропуске** (легитимные исходники `lib/src/build/` теряются) |
| 6 | **`root` в JSON — абсолютный путь машины** (P2) | `analyze-project <abs>` vs `analyze-project .` | `data.root = "/var/folders/…/determinism"` против `"/private/var/…/determinism/."`; **один проект — разный JSON** в зависимости от записи пути; утечка локального пути |
| 7 | **Вывод — только «pretty» JSON** без компактного/потокового режима: 27,7 МБ (bloc, 616 файлов), 98,5 МБ (riverpod, 1 295 файлов) (P3) | `analyze-project` на реальных репозиториях | стоит добавить `--compact`/NDJSON и фильтры полей |
| 8 | Все прочие коды выхода соответствуют документу | неизвестная команда → 2, нет пути → 2, несуществующий путь → 3, отсутствующий конфиг → 3, неверный `--format` → 2 | соответствует |

Позитив: детерминизм между запусками (идентичный вывод), симлинк на файл внутри корня поддержан (C02),
пробелы и кириллица в путях (C05) обрабатываются, лимиты входа и коды выхода — по документации.

**[стат.]** Обход каталогов использует `DirEntry::file_type()` (без следования по ссылкам) и затем `fs::read_dir(path)` по пути: между проверкой и чтением каталог теоретически можно заменить symlink'ом (TOCTOU). Для локального запуска на своём дереве риск мал; для анализа **недоверенного checkout в CI** стоит открывать каталоги через дескриптор (`openat` + `O_NOFOLLOW`). Чтение файлов уже привязано к проверенной цели и лимитируется `take(max+1)`.

---

## 9. `dartscope-resolve`, pubspec и `package_config` (998 LoC + pubspec-часть `parse`)

**[CI]** (macOS, release-CLI; `pubspec`/`pubspec-config` на 11 краевых случаях) — все завершились без паник и
быстро (≤ 15 мс), диагностики адекватны:

| Вход | Диагностики |
| --- | --- |
| табуляция вместо пробелов | `pubspec_invalid_yaml`, `pubspec_invalid_indentation` |
| якоря + merge-ключ `<<` | `pubspec_unsupported_yaml_alias` ×2, `pubspec_invalid_environment` |
| «alias bomb» (6 уровней ×10 ссылок) | 6 × `pubspec_unsupported_yaml_alias`, **без раскрытия** алиасов |
| дублирующиеся ключи | `pubspec_duplicate_key` ×2 |
| нетипичные формы зависимостей (`path`, `git`+`ref`+`path`, `hosted`+`version`, пустое значение, `any`) | без диагностик |
| огромные числа/нестрогие скаляры | без диагностик |

**Ложное срабатывание на реальном файле [CI].** Пустая секция `flutter:` (в `pubspec.yaml` официального репозитория
`flutter/samples`, `ios_app_clip/`) даёт `pubspec_invalid_flutter_configuration` уровня Error, хотя Flutter её принимает
(§7, п. 5). `parse_flutter` не различает «ключ без значения» и «значение неверного типа». Рекомендация: трактовать `null` как
пустую конфигурацию (как в `flutter_manifest.dart`), а для прочих нарушений использовать Warning, если диагностика не мешает
построить модель.

**`package_config.json` (C09) и граничные URI (C10) [CI]** — 8 вариантов конфигурации и 16 импортов:

| Проба | Результат |
| --- | --- |
| корректный `package_config` (относительные `rootUri`), а также `packageUri` без завершающего `/` | `package:dep/dep.dart` → `resolved` (`dep/lib/dep.dart`), `package:app/util.dart` → `resolved` |
| `rootUri`, выходящий за корень (`../../../../etc`) | `resolved_external`, **целевой путь не раскрывается** (пусто); обхода каталогов нет |
| абсолютный `file:///opt/dep` | `resolved_external` |
| `rootUri` с `%`-кодированием (`../d%65p`) | декодируется и разрешается верно (`dep/lib/dep.dart`) |
| дубликаты имён пакетов, `configVersion: 3` (`packages: []`), усечённый JSON | `invalid_configuration` для всех ссылок, exit 0, без паник |

Граничные URI (`uri-graph`): `dart:io` → `external`; `http://…` и `file:///…` → `unsupported_scheme`; `./b.dart`, `sub/../b.dart`,
`package:app/b.dart`, `r'b.dart'`, `import … deferred as` — `resolved`. Мелкие отклонения (P3): `'b%20c.dart'` **не декодируется**
(`missing_target` `lib/b%20c.dart`, хотя файл `b c.dart` существует, а буквальная форма `'b c.dart'` разрешается); пустой
импорт `''` даёт `missing_target` с целью `lib` (каталог), `'   '` — цель `lib/   `, вместо диагностики недопустимого URI;
`../../../../etc/passwd.dart` нормализуется с усечением лишних `..` до `etc/passwd.dart` (классификации «выход за корень» нет);
`package:app/../secret.dart` нормализуется в `secret.dart` в корне проекта — граница `lib/` (`packageUri`) не проверяется
(существующий файл по такому пути, вероятно, был бы `resolved`; не проверялось).

В целом по коду и по прогону **[стат.]/[CI]** `resolve` написан аккуратно: RFC 3986-разрешение через `uriparse`, проверки
вложенности (`is_relative_uri_path_inside_root`), пересечения `packageUri`/корней, валидация `generated`/`generatorVersion`/
`languageVersion`; единственный `expect` обоснован предшествующей проверкой.

**Зависимость `uriparse 0.6.4`** — последняя версия опубликована **2022-03-18** (по данным
[crates.io API](https://crates.io/api/v1/crates/uriparse)), дальнейших релизов нет; крейт отвечает за
безопасность разрешения путей (traversal). Стоит зафиксировать политику (вендоринг/замена, например `url`
или собственный RFC 3986-модуль) и внести в `dependency-quality.md`.

---

## 10. Производительность и масштабирование (P1)

### 10.1 Один файл: время растёт **квадратично** с размером **[CI]**

Release-CLI, `dartscope analyze-file` на синтетических файлах, GitHub Actions (прогон 36777676975, `runtime-long`; одна и та же
батарея на macOS 15 arm64 и Ubuntu 24.04 x86-64):

| Вход | Размер | Время, macOS | Время, Linux | Пик RSS, macOS |
| --- | --- | --- | --- | --- |
| 500 классов (по 5 строк: поле, конструктор, метод) | 41 КБ, 2 500 строк | **0,18 с** | **0,11 с** | 6 МБ |
| 2 000 классов | 171 КБ, 10 000 строк | **2,17 с** | **1,69 с** | 17 МБ |
| 8 000 классов | 705 КБ, 40 000 строк | **29,6 с** | **26,9 с** | 60 МБ |
| 16 000 классов | 1,45 МБ, 80 000 строк | **139,6 с** | **107,9 с** | 145 МБ |
| 2 000 вызовов `fN(aN, bN);` в одном `main()` | — | 0,26 с | 0,28 с | 7 МБ |
| 8 000 вызовов | — | 4,21 с | 4,20 с | 28 МБ |
| 32 000 вызовов | — | **74,6 с** | **67,3 с** | 73 МБ |

Увеличение входа в 4 раза даёт рост времени в **12–18 раз**. Эмпирический показатель степени: на Linux — **1,97 → 2,00 → 2,00**
(500→2 000→8 000→16 000 классов; 1,95 → 2,0 для вызовов), на macOS — 1,8 → 1,9 → 2,2 (классы) и 2,0 → 2,1 (вызовы): то есть
это **O(n²)**, воспроизведённое на двух ОС и двух архитектурах. Константа `t / n²` на точках классов — (4,2–7,2)·10⁻⁷ с.
При лимите CLI **8 МиБ на файл** (`DEFAULT_INPUT_LIMITS.max_file_bytes`; ≈ 92 000 таких классов) один допустимый
сгенерированный файл (локализации, protobuf, таблицы данных) по **экстраполяции** (допущение: n² сохраняется) потребует порядка
**1–1,7 часа**. Память растёт примерно линейно (macOS: 6 → 145 МБ) и не служит предупреждением. *RSS на Linux не приводится:*
`ru_maxrss` дочернего процесса там наследует пик родителя (Python-скрипта), значения искажены.

**Воспроизведение без зонда** (после применения патча §14.1, иначе `main` не собирается):

```bash
mkdir -p /tmp/scale/lib && cd /tmp/scale
python3 - <<'PY'
n = 8000  # 500, 2000, 8000, 16000
open("lib/a.dart", "w").write("".join(
    f"class C{i} {{\n  final int f{i};\n  C{i}(this.f{i});\n  int m{i}(int a) => a + f{i};\n}}\n"
    for i in range(n)))
PY
/usr/bin/time -v <path-to>/target/release/dartscope analyze-file lib/a.dart > /dev/null
```

Время — настенное (wall-clock) по `wait4`, пик RSS — `ru_maxrss` дочернего процесса; один запуск на точку (на разогретом
раннере), поэтому абсолютные значения — порядок величины, а **форма зависимости** (×12–18 на ×4) — главный вывод.

### 10.2 Реальные репозитории **[CI]**

| Репозиторий (shallow clone) | `.dart` файлов | `analyze-project`, macOS / Linux | `lint` (5 правил), macOS / Linux | Прочее |
| --- | --- | --- | --- | --- |
| `felangel/bloc` | 616 | **0,9 с** / 0,7 с; вывод 27,7 МБ; macOS RSS 63 МБ | 0,9 с / 0,6 с | `flutter-inventory`, `uri-graph`, `graphql-contracts` — 0,6–0,9 с |
| `rrousselGit/riverpod` | 1 295 | **6,1 с** / 4,8 с; вывод 98,5 МБ; macOS RSS 184 МБ | 5,4 с / 4,6 с | `uri-graph` 7,1 / 4,7 с, `flutter-inventory` 6,1 / 4,6 с |
| `flutter/samples` | 484 | **macOS: не завершился за 240 с (SIGKILL); Linux: 223 с**, вывод 65 МБ, 23 071 декларация | macOS: SIGKILL; **Linux: 224,5 с, затем exit 6 без результата** (§7, п. 5) | macOS: `flutter-inventory`, `graphql-contracts` — SIGKILL; `uri-graph` — 233 с |
| `dart-lang/shelf` | 99 | 0,2 с (macOS) | — | (хвост вывода обрезан лимитом аннотации) |

Сопоставление: 1 295 файлов — 5–6 с, а 484 файла `flutter/samples` — **почти четыре минуты**. Размер проекта ни при чём.
**«Тяжёлые» файлы идентифицированы** — замер `analyze-file` по каждому из 484 файлов (macOS, прогон 36783493869):

| Файл | Размер | Строк | `analyze-file` |
| --- | --- | --- | --- |
| `pedometer/lib/pedometer_bindings_generated.dart` (сгенерирован `ffigen`) | 2,66 МБ | 91 729 | **> 150 с (прерван)** |
| `pedometer/lib/health_connect.dart` (сгенерирован `jnigen`) | 1,06 МБ | 31 962 | **21,8 с** |
| прочие 482 файла (наибольший — `material_3_demo/.../component_screen.dart`, 76 КБ) | ≤ 76 КБ | ≤ 2 676 | **≤ 0,34 с каждый** (в сумме ≈ 25 с) |

Два сгенерированных файла FFI/JNI-привязок дают практически всё время прогона; это типичный для Flutter-плагинов вход
(`ffigen`, `jnigen`, protobuf, graphql-кодогенерация), а не экзотика. Отношение 91 729 / 31 962 строк = 2,9 → при n² время
должно быть в 8 раз больше (≈ 180 с): это согласуется с наблюдаемыми 223–240+ с на проект. Пик RSS macOS на момент SIGKILL — всего **84–88 МБ** (на `uri-graph` — 98 МБ):
процесс не «пухнет», он упирается в процессор (CPU-bound), то есть это не утечка и не взрыв памяти, а алгоритмическая
стоимость. В проектном масштабе (100 → 400 → 1 600 **мелких** файлов) время **линейно** и мало: 0,02 → 0,03 → 0,09 с (macOS),
0,01 → 0,01 → 0,06 с (Linux); `lint` — столько же. Вывод — всегда «pretty» JSON без компактного/потокового режима:
десятки мегабайт на средний проект (65–99 МБ на 500–1 300 файлов).

### 10.3 Причина: профиль **[CI]** и чтение кода **[стат.]**

Профиль `analyze-file` (release-сборка, Ubuntu 24.04 x86-64, valgrind 3.22 / callgrind; прогон 36784291516, job
110121850920; вход — файл из 600 и 1 200 классов по 5 строк). Исключительная стоимость функций в инструкциях (Ir):

| Функция | n = 600 | n = 1 200 | Доля при n = 1 200 | Рост при удвоении n |
| --- | --- | --- | --- | --- |
| `core::slice::memchr::memchr_aligned` | 712,6 млн | 2 847,7 млн | 36,0 % | ×4,00 |
| `<CharSearcher as Searcher>::next_match` | 506,2 млн | 2 020,4 млн | 25,5 % | ×3,99 |
| `Vec<T>::from_iter` (`collect`) | 340,4 млн | 1 357,6 млн | 17,2 % | ×3,99 |
| `declaration_inventory::collect_declaration_inventory` | 166,4 млн | 669,8 млн | 8,5 % | ×4,03 |
| `memcmp` | 73,6 млн | 291,2 млн | 3,7 % | ×3,96 |
| `source_lines::span_for_byte_range` | 57,9 млн | 231,0 млн | 2,9 % | ×3,99 |
| **всего** | **2 039 млн** | **7 912 млн** | 100 % | **×3,88** |

Удвоение входа учетверяет стоимость **каждой** заметной функции — это прямое подтверждение O(n²) на уровне инструкций, а не
артефакт таймера. Атрибуция по вызывающим (inclusive-профиль и дерево вызовов, прогон 36784982608, job 110124124957):

| Функция | Inclusive Ir при n = 1 200 | Доля |
| --- | --- | --- |
| `collect_declaration_inventory` | 7 731 млн | **97,7 %** |
| **`source_lines::span_for_byte_range`** | **6 841 млн** | **86,5 %** |
| `Vec::from_iter` (сборка таблицы строк) | 6 710 млн | 84,8 % |
| `CharSearcher::next_match` (поиск `'\n'`) | 5 156 млн | 65,2 % |
| `memchr_aligned` | 2 848 млн | 36,0 % |

`span_for_byte_range` вызывается из `collect_declaration_inventory` **4 800 раз** (четыре вызова на класс при n = 1 200) и
**каждый раз** заново строит таблицу всех строк файла (`split_inclusive('\n')` + `collect::<Vec<_>>()`, ≈ 1,4 млн
инструкций на вызов при 6 000 строк) — затем ищет в ней линейно. **86,5 % всего времени `analyze-file` уходит на это одно
место.** Число вызовов растёт линейно с числом деклараций, стоимость вызова — линейно с размером файла → O(n²). Остаток (≈ 13,5 %) —
собственная стоимость `collect_declaration_inventory` (8,5 %, тоже ×4: `collect_members`/`collect_locals` для каждого владельца
заново проходят строки с нуля, `brace_depth_at` — O(длина файла) на каждый тип **[стат.]**), сравнения строк (`memcmp`,
3,7 %) и прочее; после устранения первой части именно остаток станет главным на больших файлах.

*Минимальное исправление:* один раз на файл строить `LineIndex` (`Vec<usize>` начал строк) и вычислять `(line, column)`
бинарным поиском (передавать индекс по ≈ 50 местам вызова или хранить его в контексте анализа) — по профилю это снимает
≈ 86 % стоимости; затем для членов и локалов передавать курсор и идти вперёд за один проход (остаток); ввести регрессионный
тест «n = 4× → t < 8×» (или верхнюю границу времени) на 2–4 масштабах. **Оценка (экстраполяция, не замер):** после первой
части 16 000 классов — порядка 15 с вместо 108 с (Linux), файл `pedometer_bindings_generated.dart` — порядка десятков секунд;
полностью линейное поведение требует и второй части.

### 10.4 Индекс и навигация

**[CI]** разбор и одиночный запрос по цепочке из 3 000 классов `extends` — **2,9–4,7 с** (Windows 2,9 и 4,7 с, Ubuntu 4,1 с; C3; тот же n²); 30 правок индекса
из 400 файлов — 66 мс (C5); циклы и цепочки из 1 500–3 000 элементов не вешают и не переполняют стек (§5.4).
**[стат.]** дополнительные нелинейные места — §5.5. Замеров навигации на больших проектах нет.

### 10.5 Бенчмарк репозитория не защищает от этого

`quality_benchmark.rs` использует 320 классов и 600 файлов (точки, где зависимость ещё не видна), а CI сравнивает только
«база против кандидата» без абсолютных порогов и без проверки масштабируемости: квадратичность от размера файла не
регистрируется.

---

## 11. Тесты, fuzz и процесс

* **Покрытие тестами** (по числу): 388 проходящих + 2 красных; `dartscope-lsp` — 11 unit-тестов и ни одного
  интеграционного/протокольного; `dartscope-resolve` — только inline; у бинарника LSP тестов нет совсем.
* **Fuzz**: 5 целей (`lexical_masking`, `directives`, `pubspec_package_config`, `graphql`, `uri_normalization`)
  покрывают только ранние стадии. **Не фаззятся**: инвентарь деклараций, invocations, идентификаторы/ссылки,
  лексические регионы/привязки, индекс (namespace/uri_graph/navigation/incremental), lints, Flutter-конвенции,
  CLI-обход, LSP-координаты и фрейминг. Инвариант `assert_span` слабый (нет проверки границ char и
  согласованности строка/столбец). CI выполняет **256 запусков** на цель с 2 файлами корпуса — это smoke-тест.
  Сканеры — побайтовые срезы `&str` с `expect`; для них разумна одна end-to-end цель
  `analyze_file_with_references(arbitrary_utf8)` плюс проверка инвариантов спанов (границы char, монотонность).
* **Мой мутационный прогон по CLI (результат положителен)**: `analyze-file`/`lint` на 252 реальных файлах (shelf, bloc,
  riverpod, flutter/samples, ≤ 30 КБ; мутации — усечение, вставка/дублирование/перестановка, токены Dart и Unicode: кавычки,
  `${`, `/*`, BOM, U+2028, NUL, эмодзи): **15 747 запусков на macOS и 80 351 на Linux — 0 паник, сигналов, ненулевых кодов и
  таймаутов (> 8 с)**. «Нарушения
  инварианта span'ов» (6) — все та же конвенция EOF для диагностик незавершённых литералов. Это снижает вероятность того,
  что `expect("identifier token")`-пути достижимы через публичный CLI на реалистичном вводе, но не заменяет фаззер с покрытием
  (файлы < 30 КБ, без проектного контекста, без обратной связи по покрытию).
* **Процесс**: PR #159 слит с красным CI; план/CHANGELOG/аудит-документ подают изменения как «implemented/fixed»
  без единой успешной сборки; нет защиты ветки, которая запретила бы слияние при красных обязательных проверках
  (не проверялось — нет доступа к настройкам репозитория, но факт слияния показывает отсутствие такой защиты).
* **Гейты репозитория не ловят собственные дефекты**: `cargo metadata --locked --no-deps` не проверяет
  актуальность lock-файла; `check-repository-consistency.py` не сверяет числа крейтов в документации
  (хотя проверяет «eight crates»); политика macOS закрепляет `9` тестом.

---

## 12. Документация против реальности; достоверность прошлых аудитов

### 12.1 Расхождения документации и кода **[док.]**

| Документ | Утверждение | Факт |
| --- | --- | --- |
| `dartscope-library-plan.md` DS-LSP-001 | «Status: implemented»; «diagnostics via `DartWorkspaceIndex`»; «server remains responsive under cancellation» | не компилируется (§3.1); diagnostics не публикуются; `$/cancelRequest` — заглушка |
| `server.rs` (комментарий), plan | «the incremental index itself is reused internally» | индекс создаётся заново на каждую правку |
| `dartscope-library-plan.md` «Verified Baseline» | «nine crates … matrix passed» | 10 крейтов; CI красный |
| README «Current Scope» | перечислены крейты | `dartscope-lsp` отсутствует; «All nine crates» |
| CHANGELOG | «Nine publishable Rust crates» и тут же «New optional crate `dartscope-lsp`» | противоречит сам себе |
| CHANGELOG | «`pubspec_yaml_marked.rs` no longer panics… keeps the fuzz corpus panic-free» | код не компилируется; паники не было |
| CHANGELOG | «fingerprints are now span-invariant so formatting-only edits no longer rebuild dependents» | приводит к устаревшим спанам (§5.2), красный тест |
| CHANGELOG | «share one canonical scanner instead of twelve local character classes» | остаётся ≥ 9 локальных классов; `is_identifier_byte` — мёртвый код |
| CHANGELOG/plan | «extension member fallback … `DirectImport` basis» | базис не проверяет импорт (§5.1) |
| `cli-contract.md`/README | «errors write only to stderr with stable exit codes» | паника 101 на не-UTF-8 аргументе и закрытом stdout |
| `support-matrix.md` | «Blocking workspace tests …; blocking macOS portability…» | эти гейты красные на `main` |
| `rust-code-standards.md` | `lib.rs` — «thin»; функция ≤ 100 строк; файл ≤ 1 200 | `core/lib.rs` 964 строки типов; `rebuild` 235 строк; `incremental.rs` 1 885 |

### 12.2 Что не подтвердилось в `audit-findings-2026-09-25-detailed.md`

| Утверждение | Результат проверки |
| --- | --- |
| «Все найденные дефекты — исправлены в этом же изменении»; ожидаемые `cargo check/test/clippy` — pass | Изменение **ломало сборку** (§2); ни одна из заявленных команд не была запущена (документ сам это признаёт в §0, но выводы сформулированы как «ожидается pass») |
| «43 741 LoC (`cargo wc`)» | `cargo wc` не существует; фактически ≈ 33,9 тыс. строк `src` + ≈ 12 тыс. тестов в крейтах |
| «6 `allow` (`too_many_lines`, `cognitive_complexity`)» | таких нет; есть 9 × `too_many_arguments` и (после PR) 4 × `dead_code` |
| «`incremental.rs` 1886 LoC — не дробить; порог 600 LoC» | порог стандарта — 1 200 строк, после которого разбиение обязательно |
| «Непокрытый пробел: неподдержка не-ASCII идентификаторов» (план фазы 3) | **не дефект**: грамматика Dart допускает только ASCII (`LETTER: 'a'..'z' \| 'A'..'Z'`) — [спецификация, лексика](https://groups.google.com/a/dartlang.org/g/misc/c/RhXKOTB0hfI); [Issue gitgalaxy#3954](https://github.com/squid-protocol/gitgalaxy/issues/3954) |
| «+650 строк документации» | файл — 341 строка |
| Пробел: «populated goldens» | фикстуры добавлены в PR #159 (и проходят) |

Фаза-1 аудит (английский) заявляет реальный запуск `cargo` (384 теста, локальный Rust 1.88); проверить это задним числом нельзя, но его выводы согласуются с данными §2.6 для неизменённых частей кода.

---

## 13. Пробелы реализации (по отношению к заявленному и к типичным ожиданиям)

1. **LSP**: нет рабочего протокольного слоя (§3); нет workspace-модели (сканирование корня, `didChangeWatchedFiles`,
   `pubspec.yaml`/`package_config.json`), publishDiagnostics, cancellation, прогресса, completion/rename/workspace symbols
   (последние — честно в списке follow-up плана).
2. **Инвентарь**: enum-константы; top-level getter/setter; функции и методы с type-параметрами `<T>`; функциональный тип
   результата; record-возврат (§4.2).
3. **Навигация**: многоуровневое наследование; `on`-тип extension и проверка импорта; `mixin … on` как отдельное понятие;
   локальные функции как привязки (в плане признано); каскады, null-aware, паттерны, записи.
4. **Инкрементальность**: парсер не инкрементален (повторный полный разбор файла); LSP не использует инкрементальный индекс;
   нет кэша контекста разрешения на снимок.
5. **CLI**: нет команды навигации (`find-definition`/`find-references`) — возможности библиотеки недоступны процессу;
   `lint` без конфигурации ничего не делает; нет вывода для пропущенных/отклонённых входов (symlink, `build/`, не-UTF-8) как
   диагностик вместо отказа; нет `--root-relative` детерминированного `root`.
6. **Lint**: правила ограничены import/naming/parts/orphan; нет suffix/glob-исключений (`*.g.dart`); нет покрытия `export`;
   нет правила на неиспользуемые импорты/зависимости pubspec.
7. **Документация крейтов**: единый корневой README для 10 крейтов на crates.io; у `dartscope-lsp` нет страницы в `docs/`.
8. **Релиз**: MSRV = последний стабильный (1.95) — для библиотеки это высокий барьер (let-chains, `is_multiple_of`);
   `0.1.0` не выпускался — гейты, которые должны защищать релиз, красные.
9. **Производительность и устойчивость к размеру входа**: нет бюджета времени/операций и нет защиты от квадратичного роста
   (лимит 8 МиБ на файл допускает многочасовой разбор, §10); нет компактного/потокового вывода (десятки МБ JSON на проект, §8).

---

## 14. План исправлений (приоритизированный)

### 14.1 Путь к зелёному CI (P0) — проверено на CI

Патч `audit-2026-09-30-unblock.patch` (приложен к этому документу; применять из корня репозитория
`git apply docs/development/audit-2026-09-30-unblock.patch`, затем `cargo fmt --all`) содержит:

1. `Cargo.lock` — запись `dartscope-lsp` (точная разница из §2.3);
2. `pubspec_yaml_marked.rs` — структурно полная `if let Some(key_node) = pending_key.take() { … } else { … }`
   вместо невалидного `let … else` (без `expect`, без недостижимой диагностики, поведение прежнее);
3. `lexical.rs` — удалены мёртвая `is_identifier_byte` и неиспользуемый реэкспорт `StringLiteralRange`;
4. `literals.rs` — исправлен **сам тест** (`find_string_literal_start("foo$r'bar'", 0) == Some(5)`);
5. `navigation/members.rs` — `collapsible_if` (let-chain);
6. `incremental.rs` — возвращена `&declaration.span` в `top_level_declaration_facts` (устраняет §5.2);
7. `ci.yml`, `tools/report_benchmark_regressions.py`, `tools/tests/test_macos_portability_policy.py` — «9» → «10»;
8. LSP — минимальные исправления компиляции (`from_snapshot(&…)`, `derive(Default)` у `Position/Range/Diagnostic`,
   заём в `document_symbols`, неиспользуемые импорты).

**Результат проверки первого патча отдельно** (macOS arm64, Rust 1.95.0, патч + `cargo fmt --all`; прогон 36779944962):

| Команда | Итог |
| --- | --- |
| `git apply` | чисто |
| `cargo fmt --all -- --check` (после `cargo fmt --all`) | успех; суммарный дифф «патч + форматирование» — 17 файлов, +327/−127 строк |
| `cargo check --workspace --all-targets --locked` | **успех** (включая `dartscope-lsp`) |
| `cargo check -p dartscope --all-features --locked` / `--no-default-features --locked` | **успех** |
| `cargo test --workspace --locked --no-fail-fast` | всё зелёное, **кроме 3 unit-тестов LSP**: `coordinates::tests::handles_crlf`, `server::tests::diagnostics_published_for_unsupported_syntax`, `server::tests::did_open_and_definition_round_trip` |
| `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --locked` | успех |
| `python -m unittest discover -s tools/tests` | 22/22 |
| `cargo package --workspace --locked --allow-dirty --no-verify` | 10 архивов |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | **красный: 6 ошибок** (5 в LSP: `needless_update` ×2, `collapsible_if` ×2, `result_unit_err`; 1 — `nonminimal_bool` в моей правке теста) |

**Три красных LSP-теста — дефекты самих тестов [CI]** (после правки ожиданий патчем `lsp-test-fixes` все три зелёные, см.
таблицу ниже): 

* `coordinates::tests::handles_crlf` — неверная нумерация байтов в ожиданиях: в `"a\r\nb\r\nc"` байт 3 — это `b`, первая буква
  строки 1; реализация возвращает `(1, 0)` — верно, тест требовал `(0, 1)`;
* `server::tests::diagnostics_published_for_unsupported_syntax` — входной `class Foo { Foo.new(); }` — обычный безымянный
  конструктор; «concise»-форма Dart 3.13 — это ведущее `new` (`new();`), как в собственном тесте крейта `parse`
  (`reports_dart_3_13_constructor_syntax_without_fabricating_members`);
* `server::tests::did_open_and_definition_round_trip` — ждёт определение члена `bar` у приёмника-выражения `Foo().bar()`, но
  вывод типа такого приёмника не реализован (в плане «receiver inference» вне scope); тест заменён на определение класса по
  вызову `Foo()`. Исходный замысел (член через выражение-приёмник) остаётся **пробелом функциональности**.

**Второй и третий патчи.** `audit-2026-09-30-clippy-followup.patch` закрывает остальные ошибки Clippy (6 из первой проверки и
ещё 2 `unnecessary_cast` в тестовом коде LSP, которые проявились только после устранения первых — они были скрыты до
компиляции lib-части; обнаружены повторной проверкой, исходная версия патча их пропускала). `audit-2026-09-30-lsp-test-fixes.patch`
правит ожидания трёх тестов выше.

### 14.1а Приложенные патчи (в `docs/development/`) и итог проверки

> **Статус (2026-10-01).** Патчи применены в коммите `8223171` и удалены из дерева; их текст сохранён в истории
> (`git show 8a7aa22:docs/development/audit-2026-09-30-unblock.patch`, то же для трёх остальных). Описание ниже — это
> протокол их проверки на момент аудита.

Порядок применения (из корня репозитория):

```
git apply docs/development/audit-2026-09-30-unblock.patch
git apply docs/development/audit-2026-09-30-clippy-followup.patch
git apply docs/development/audit-2026-09-30-lsp-test-fixes.patch
cargo fmt --all
git apply docs/development/audit-2026-09-30-regression-tests.patch   # необязательно
cargo fmt --all
```

| Файл | Назначение | Статус проверки |
| --- | --- | --- |
| `audit-2026-09-30-unblock.patch` | минимальный путь к компилируемому workspace: lock, `pubspec_yaml_marked`, мёртвый код, тест `literals`, `incremental.rs`, число крейтов, компиляция LSP | **CI**: macOS, Ubuntu, Windows (прогоны 36779944962, 36785287092, 36785607267) |
| `audit-2026-09-30-clippy-followup.patch` | оставшиеся ошибки Clippy (LSP, тест `literals`, 2 × `unnecessary_cast`) | **CI**: `cargo clippy --workspace --all-targets --locked -- -D warnings` зелёный |
| `audit-2026-09-30-lsp-test-fixes.patch` | ожидания трёх красных тестов LSP (дефекты тестов) | **CI**: `cargo test --workspace --locked` — 401 passed, 0 failed, 1 ignored |
| `audit-2026-09-30-regression-tests.patch` | 5 новых тестовых файлов: 9 executable-спецификаций с `#[ignore = "…"]` (пока дефекты не исправлены), 2 «сторожа» для §5.2 (проходят после первого патча), 8 проходящих проверок (5 на циклы/глубокие цепочки, 2 навигации, 1 лексики) | **CI**: набор по умолчанию — 411 passed, 0 failed, 10 ignored; с `--include-ignored` 9 спецификаций падают именно так, как описано |

**Итоговая проверка всех патчей подряд** (Rust 1.95.0; прогоны 36784982608 (macOS), 36785287092 (macOS, Ubuntu),
36785607267 (Windows)): 

| Стадия | `fmt --check` | `clippy -D warnings` | `cargo test --workspace --locked --no-fail-fast` |
| --- | --- | --- | --- |
| unblock + clippy-followup + lsp-test-fixes, затем `cargo fmt --all` | успех (macOS, Ubuntu) | **успех** (macOS, Ubuntu) | **401 passed, 0 failed, 1 ignored** (88 тестовых бинарников; macOS, Ubuntu) |
| + regression-tests, затем `cargo fmt --all` | успех (macOS, Ubuntu) | **успех** (macOS, Ubuntu) | **411 passed, 0 failed, 10 ignored** (93 бинарника; macOS, Ubuntu) |
| `--include-ignored` для 5 файлов регрессионных тестов | — | — | циклы 5/5 ✓; инкрементальность 2/2 ✓; навигация 2 ✓ + 4 ✗; Flutter 1 ✗; парсер 1 ✓ + 4 ✗ — 9 ✗ ожидаемы (macOS, Ubuntu) |
| скрипты репозитория на дереве после патчей (чистая копия без файлов зонда) | `check-repository-consistency.py`, `check-workflow-policy.py`, `check-dependency-policy.py` — **успех**; `tools/tests` 22/22 (macOS, Ubuntu) | | |

**Windows** (прогон 36785607267, `windows-2025`, checkout с `core.eol=lf`): все четыре патча применяются; `fmt --check` и
`clippy -D warnings` — успех; `cargo test --workspace` — **394 passed, 0 failed, 1 ignored**, с регрессионным набором
**404 / 0 / 10** (на 7 тестов меньше, чем на Unix, — это тесты под `cfg(unix)`; такая же разница наблюдалась до патчей:
381 против 388). Скрипты-гейты в этом Windows-прогоне **не выполнены** (огрех моей подготовки копии дерева, не дефект репозитория)
— они проверены только на macOS и Ubuntu.

**Важно для Windows-разработчиков [CI]** (прогон 36785607267, job `applycheck`). При стандартном checkout Git for Windows
(`core.autocrlf=true` в системном gitconfig) файлы `*.patch` получают CRLF: в `.gitattributes` для них нет правила, действует
`* text=auto`. В результате **`git apply` отвергает каждый из четырёх патчей целиком** (`patch does not apply` даже для файлов с
`eol=lf`), а `git apply --ignore-whitespace` применяет все четыре (проверено; для добавляемых файлов git предупреждает о
лишних пробелах/CR). Обходы: `git apply --ignore-whitespace …` либо выкачать патчи с LF. Рекомендация: добавить в
`.gitattributes` строку `*.patch text eol=lf` — сейчас на Windows-checkout также `tools/*.py` получают CRLF (`w/crlf`).

Что **не** проверялось: полный `ci.yml` на дереве с патчами (джобы `fuzz` и `benchmark_report`, сборка целей `cargo fuzz`
на nightly, сравнение бенчмарка «база против кандидата»), а также `release.yml`; это нужно сделать в PR.

### 14.2 Дальше по приоритетам

| Приоритет | Задача | Раздел |
| --- | --- | --- |
| **P0** | Применить патчи §14.1–14.1а (`unblock`, `clippy-followup`, `lsp-test-fixes`; `regression-tests` — по желанию) и прогнать полный `ci.yml` в PR; дополнить гейты: `cargo metadata --locked` без `--no-deps` или `cargo check --locked`, число крейтов из `release-crates.txt`, **защита ветки** (обязательные проверки до merge) | §2, §11 |
| **P0/P1** | LSP: либо довести (camelCase, ответ на каждый request и `-32xxx` на ошибки, clamp/без паники в `did_change`, корректный percent-кодированный URI, `selectionRange` внутри `range`, `includeDeclaration`, `publishDiagnostics`, workspace-модель, протокольные тесты), либо **исключить из 0.1** (убрать из `release-crates.txt` и umbrella, статус плана → `in_progress`) | §3 |
| **P1** | Навигация: убрать extension-fallback без проверки `on`-типа и импорта (или возвращать `Ambiguous`/`ExternalUnindexed`); транзитивное наследование | §5.1, §5.3 |
| **P1** | Flutter: фильтровать `kind == Class` в `derive_flutter_file_hints`/`ecosystem.rs`; перестать перегружать `extends`/`mixes_in` (ввести отдельные поля и версию схемы) | §4.5, §6 |
| **P1** | BOM; функции и методы с `<T>` и функциональным типом результата (ложная декларация `Function`); enum-константы; top-level accessors | §4.1, §4.2 |
| **P1** | Производительность: единая `LineIndex` на файл (бинарный поиск вместо пересборки таблицы на каждый спан — по профилю это ≈ 86 % стоимости, §10.3); один проход по телу владельца; `HashMap` вместо `iter().find`; кэш контекста разрешения по `generation`; регрессионный тест масштабируемости (2–4 размера, отношение времени) в CI; предел времени/размера файла с диагностикой вместо многочасового счёта | §10, §5.5 |
| **P1/P2** | CLI: `args_os`, корректный EPIPE, symlink/`.symlinks` и не-UTF-8 как диагностика + пропуск, детерминированный `root`, диагностика пропущенных каталогов | §8 |
| **P1** | Pubspec/lint: пустой `flutter:` — не ошибка (как в `flutter_manifest.dart`); не прерывать `lint` из-за Error-диагностик отдельных pubspec (выдавать их как findings и продолжать); не тратить 4 минуты на анализ до отказа (проверять pubspec до разбора исходников) | §7 (п. 4–5), §9 |
| **P2** | Lint: `export`/conditional-import, suffix-исключения (`*.g.dart`), сегментные префиксы, предупреждение «0 правил», SARIF `uri` percent-encoding | §7 |
| **P2** | Fuzz: end-to-end цель `analyze_file_with_references`; инварианты границ char и строк/столбцов; корпус из реальных файлов; 10 000+ запусков по расписанию | §11 |
| **P3** | `uri-graph`: `%`-декодирование относительных импортов, диагностика пустого/пробельного URI, классификация «выход за корень», граница `packageUri` для `package:app/../x` | §9 |
| **P3** | `.gitattributes`: `*.patch text eol=lf` (и правила для `*.py`/`*.sh`) — иначе на Windows-checkout `git apply` отвергает патчи | §14.1а |
| **P3** | Разбить `incremental.rs` (1 885 строк) и `rebuild`; убрать `allow(dead_code)`/`too_many_arguments`; синхронизировать документацию (§12.1); заменить/зафиксировать `uriparse` | §5.5, §9, §12 |

---

## 15. Приложения

### 15.1 Доказательная база (где искать)

Результаты получены прогонами временного зонда на ветке `arena/01a0f406-dartscope` (коммиты `c5dc807…5f742b6`; скрипты:
`audit-probe/`, workflow: `.github/workflows/audit-probe*.yml` — **в итоговом изменении удалены**, но сохранены в истории
ветки). Восстановление: `git checkout 5f742b6 -- audit-probe .github/workflows/audit-probe-final.yml` (скрипты всех
раундов и workflow заключительного раунда), `git checkout ca945ce -- .github/workflows` (workflow ранних раундов). Хеши
существуют, пока существует ветка аудита (при squash-merge они останутся только в ней). Прогоны (GitHub Actions):

| Что | Run | ОС |
| --- | --- | --- |
| CI `main` / Release / плановый CI | 36185507113 / 36185507160 / 36431529044 | ubuntu, windows, macOS |
| «как закоммичено» + состояние сборки, fmt, clippy, doc | 36777676975 (`baseline`) | macOS arm64 |
| тесты после нейтрального исправления компиляции | 36777081916 (`tests`), 36777676975 (`tests`) | Windows / macOS |
| батарея Dart-входов, проекты, CLI | 36777676975 (`runtime`) | macOS arm64 |
| Rust-пробы навигации, инкрементальности, Flutter, парсера | 36777676975 (`tests`) | macOS |
| циклы/глубокие цепочки, релизные гейты | 36777989607 (`extra`) | Windows |
| верификация патча §14.1 | 36779944962 (`unblock`) | macOS, Windows |
| масштабирование, мутационный fuzz, реальный корпус (shelf/bloc/riverpod/samples) | 36777676975 (`runtime-long`: job 110100168263, 110100168220) | macOS arm64 (31 мин), Ubuntu 24.04 (33 мин) |
| циклы/глубокие цепочки и релизные гейты, повтор | 36779605707 (`extra`) | Ubuntu, Windows |
| протокольный прогон LSP по проводу (G01–G16, §3.7) | 36783493869 (job 110119249858, `lsp`) | macOS arm64 |
| матрица форм A3, `package_config`/`uri-graph` (C09/C10), замер `analyze-file` по каждому файлу `flutter/samples` | 36783493869 (job 110119250277, `runtime_final`) | macOS arm64 |
| первая проверка всех патчей подряд (выявила 2 `unnecessary_cast`) | 36783493869 (job 110119250055, `patches`) | macOS arm64 |
| повторная проверка, профиль callgrind | 36784291516 (job 110121850595, 110121850920) | macOS arm64, Ubuntu 24.04 |
| патчи + тесты LSP, атрибуция профиля по вызывающим | 36784982608 (job 110124125405, 110124124957) | macOS arm64, Ubuntu 24.04 |
| итоговая проверка патчей на трёх ОС, гейты репозитория | 36785287092 (jobs 110125114489, 110125114627, 110125114291), 36785607267 (jobs 110126155447, `applycheck`) | macOS, Ubuntu, Windows |

### 15.2 Что не выполнено или выполнено частично (без домыслов)

* **Дифференциальная проверка против `package:analyzer` — не выполнена**: установка упала на `dart pub get` (analyzer 6.11.0
  требует пакет `_macros`, которого нет в Dart SDK 3.13.5; `analyzer ^14.4.0` имеет другой API). Харнес остаётся в истории
  ветки (`audit-probe/diff/`, коммит `70cb749`) и требует адаптации. Следствие: корректность инвентаря сверена только с
  ожиданиями из README и моими минимальными примерами (34 формы A3 и др.), а не с эталонным парсером Dart; возможны
  пропуски, которых я не искал.
* **Полный `ci.yml` и `release.yml` на дереве с патчами не запускались.** Проверены по отдельности `fmt`, `clippy`, `test`,
  `doc`, `package`, скрипты политик и `tools/tests`; **не** проверены джобы `fuzz` (сборка целей `cargo fuzz` на nightly),
  `benchmark_report` (сравнение «база — кандидат») и сам `release.yml`. Fuzz-цели (`cargo fuzz`) не запускались.
* **LSP**: прогон по проводу выполнен на macOS, debug-сборка после компиляционного патча, скриптовым клиентом — не VS Code /
  Neovim; чтение `rootUri` (п. 2 §3.2) и часть пунктов §3.3–3.6 остаются **[стат.]**; поведение на Linux/Windows (в частности на
  огромный `Content-Length`) не проверялось.
* **Реальный корпус**: `flutter/samples` на Linux — только `analyze-project` и `lint` (хвост отчёта обрезан лимитом аннотации
  4096 символов), на macOS — SIGKILL по таймауту; у `shelf` (macOS) виден только `analyze-project`. Классификация
  53 + 297 naming-предупреждений на «истинные/ложные» не проводилась. RSS на Linux не использован (искажён наследованием пика
  родителя).
* **Производительность**: профиль снят на синтетическом входе (600 и 1 200 классов) на Linux; эффект исправления не
  измерялся (оценки в §10.3 — экстраполяция); вклад `collect_members`/`collect_locals` отдельно не атрибутирован.
* **Гейты репозитория** (`check-repository-consistency.py` и др.) проверены на macOS и Ubuntu; на Windows в итоговом прогоне не
  выполнялись (огрех подготовки копии дерева в зонде).
* Живая загрузка SARIF в GitHub Code Scanning не проверялась (§7).

### 15.3 Сводка по количеству

| Категория | Число |
| --- | --- |
| Блокеры сборки/CI (P0) | 6 групп (§0) |
| Функциональные дефекты, подтверждённые исполнением (P1/P2) | 18 (квадратичная стоимость анализа файла; ложная Error на пустом `flutter:` (падение `lint` на `flutter/samples`); функции и методы с `<T>`/функциональным типом результата выпадают из инвентаря; enum-константы и top-level accessors; BOM; ложные Flutter-виджеты; выдуманные цели навигации; устаревшие спаны; перегрузка `extends`/`mixes_in`; паника на не-UTF-8 аргументе; паника на EPIPE; отказ на symlink-каталоге; отказ на не-UTF-8 файле; молчаливый пропуск каталогов; `root` с абсолютным путём; ложный `unterminated_string`; ложные naming-срабатывания; обход lint через `export`/conditional) |
| Дефекты LSP (компиляция, статический разбор, прогон по проводу) | 25 пунктов (§3), из них 13 подтверждены прогоном по проводу (§3.7) |
| Пробелы реализации | 9 (§13) |
| Расхождения документации/прошлых аудитов | 12 + 7 (§12) |

### 15.4 Быстрое воспроизведение (после сборки `cargo build --release -p dartscope-cli`)

```sh
# BOM скрывает первую декларацию (§4.1)
printf '\xef\xbb\xbfclass First {}\nclass Second {}\n' > a.dart
dartscope analyze-file a.dart | python3 -c "import json,sys; print([d['name'] for d in json.load(sys.stdin)['data']['declarations']])"   # ['Second']

# паника на не-UTF-8 аргументе (§8.1) и закрытом stdout (§8.2)
dartscope analyze-file $'\xff.dart'; echo "exit=$?"                        # exit=101
dartscope analyze-file big.dart | head -c 16 >/dev/null; echo "exit=${PIPESTATUS[0]}"   # exit=101

# отказ всего прогона из-за symlink на каталог — как в ios/.symlinks (§8.3)
mkdir -p p/ios/.symlinks/plugins && ln -s /tmp p/ios/.symlinks/plugins/plugin
dartscope analyze-project p; echo "exit=$?"                                 # exit=3

# функции и методы с type-параметрами не инвентаризируются (§4.2)
printf 'T first<T>(List<T> items) => items.first;\nclass Box {\n  R map<R>(R a) => a;\n}\n' > g.dart
dartscope analyze-file g.dart | python3 -c "import json,sys; print([d['name'] for d in json.load(sys.stdin)['data']['declarations']])"   # нет `first` и `map`

# lint без конфигурации ничего не проверяет (§7.1)
dartscope lint . | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['summary'])"   # enabled_rules: 0

# ложные Flutter-виджеты (§6.1): extension … on Widget попадает в widgets[]
printf "import 'package:flutter/widgets.dart';\nextension WidgetX on Widget { Widget padded() => this; }\n" > w.dart
dartscope analyze-file w.dart | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['flutter']['widgets'])"
```

---

## 16. Статус исправлений (2026-10-01)

Исправления сделаны в ветке аудита `arena/01a0f406-dartscope`. Каждая стадия проходила CI-цикл на трёх ОС (macOS 15,
Ubuntu 24.04, Windows Server 2025): `cargo fmt`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`; на macOS дополнительно `cargo doc -D warnings` и проверки feature-комбинаций.
Состояние на конец работы: clippy — 0 замечаний; тесты — **537 passed / 0 failed / 2 ignored** на Linux и macOS и
**529 / 0 / 2** на Windows (прогон GATES_RUN; игнорируемые — прежний тест и информационный `reference_pass_scaling`). На финальном дереве, без
временного CI-контура, отдельно прошли гейты постоянного `ci.yml`, которые выполняются на одном раннере: `cargo fmt
--check`, `check-repository-consistency.py`, `check-workflow-policy.py`, юнит-тесты `tools/tests` (22), `check-dependency-policy.py`,
`actionlint 1.7.12`, `cargo check --workspace --all-targets --locked`, `clippy -D warnings`, `cargo doc -D warnings`, проверки
feature-комбинаций, тесты моста `fuzzing`, `cargo package` (10 архивов) и `cargo machete 0.9.2`. **Не выполнялись:**
`cargo audit`, nightly-задача `fuzz` (cargo-fuzz) и отчёт `benchmark_report`; запуск `ci.yml` через `workflow_dispatch`
недоступен токену бота (HTTP 403), так что полный постоянный CI запустится на pull request. Для сравнения: на `5df9945`
workspace не собирался.

### 16.1 Статус по разделам

| Раздел | Статус | Что сделано |
| --- | --- | --- |
| §2 Сборка, CI, гейты | **исправлено** | `8223171`: lock-файл, `pubspec_yaml_marked`, мёртвый код, тест `literals`, fingerprint со спанами, число крейтов 9 → 10, fmt и clippy (прогон 36816462356). Гейт `cargo check --locked` в `ci.yml` уже был: не хватало обязательных проверок до merge — это настройка репозитория (branch protection), в коде её нет |
| §3 `dartscope-lsp` | **исправлено; пределы записаны** | `a5e6889`, `51fd24e`, `63dba80` (прогоны 36825390114, 36826523713): имена в camelCase, ответ на каждый запрос и коды `-32700/-32600/-32601/-32602/-32002`, код выхода по `shutdown`/`exit`, предел `Content-Length`, `publishDiagnostics`, `LineIndex`, URI клиента в ответах, корректный percent-декодинг, clamp в `didChange`, `includeDeclaration`, `selectionRange` по имени, вложенный outline, инкрементальный индекс, кэш контекста, процессные тесты на реальном бинарнике; паника внутри обработчика даёт ответ `-32603`, паника при анализе документа выводит его из индекса с предупреждением `analysis_failed` до следующей правки. Остаётся: workspace = только открытые документы (без `pubspec.yaml`), нет отмены запросов, нет workspace symbols (`dartscope-library-plan.md`, DS-LSP-001) |
| §4.1 BOM | **исправлено** | BOM — преамбула файла: строки, колонки и первая декларация; BOM в `pubspec.yaml` (`e152d41`) |
| §4.2 Инвентарь деклараций | **исправлено** | enum-константы (`Field`), top-level `get`/`set`, generic- и функциональные типы результата, именованные аргументы больше не «локальные переменные» (`e152d41`, `24a17e0`) |
| §4.3 Интерполяция `${…}` | **исправлено** | сканирование ограничено 4096 байтами и глубиной 8 (`e152d41`) |
| §4.5 `extends`/`mixes_in` | **исправлено (решение по контракту)** | `extends` — только `extends` класса, `mixes_in` — `with`, новое аддитивное `on_types`; запись в `json-contracts.md` и CHANGELOG |
| §5.1 Extension-навигация | **исправлено** | кандидат только при видимом extension (импорт с префиксом и без, не deferred, не скрыт) и подходящем `on`-типе; интерфейсный член всегда приоритетнее (`c6490ee`, прогон 36819460949) |
| §5.2 Инкрементальный снимок | **исправлено** | спаны возвращены в факты (`8223171`); затем выяснилось, что этого мало (кэшированное разрешение хранит полный `declaration_span` цели, а цель может быть членом), и инвалидация стала сравнивать все видимые другим файлам декларации — с `extends`/`mixes_in`/`on_types` и обоими спанами (`e1a4682`); тесты эквивалентности снимка и stateless-анализа, в том числе случайные последовательности правок |
| §5.3 Наследование | **исправлено** | порядок поиска Dart на любую глубину (предел 128, защита от циклов) |
| §5.5 Сложность | **частично** | хеш-индексы членов, кэш контекста разрешения по поколению индекса (LSP). Не сделано: разбиение `incremental.rs` (1 885 строк) и `rebuild` |
| §6.1 `extension … on Widget` | **исправлено** | Flutter-находки только для `Class` (`5fada6c`) |
| §6.2 Транзитивные виджеты, `imports_official_flutter` на каждый вызов | **не исправлено** | не затрагивалось |
| §7 Lint, SARIF | **частично** | исправлено: `naming_convention` пропускает имена с `$`; `forbidden_import` и `layer_boundary` видят `export`, первый — и conditional-импорты; `orphan_file` сообщает о несуществующей точке входа, пустой `entry_points` при включённом правиле — ошибка конфигурации (exit 5); SARIF `uri` кодируется, у результатов без span есть `region` (`080d7e0`, прогон 36820537970). Оставлено по решению: `lint` без `--config` ничего не запускает (описано в `lint-cli.md`); Error-диагностика проекта прерывает `lint` кодом 6 (описано); префиксы — строковые (в `lint-cli.md` сказано писать `lib/ui/`). Не сделано: исключения по суффиксу (`*.g.dart`), сегментные префиксы, предупреждение «0 правил» |
| §8 CLI | **частично** | исправлено: не-Unicode аргумент (exit 2), закрытый stdout, symlink-каталоги Flutter (`.symlinks`, `.plugin_symlinks`) пропускаются, не-UTF-8 `.dart` в `analyze-project` — предупреждение `input_file_not_utf8` и пропуск, `build`/`coverage`/`target` пропускаются только вне `lib`, `bin`, `test` и т. п. Оставлено по решению: `data.root` — абсолютный путь, вывод только «pretty», symlink-политика fail-closed. Не сделано: диагностика о пропущенных каталогах, TOCTOU-замечание |
| §9 `resolve`, pubspec | **исправлено; одна рекомендация записана** | пустой `flutter:` принимается (`e152d41`); P3 `uri-graph`: `%`-декодирование, пустой/пробельный URI, выход за корень, `package:app/../x` (`63dba80`, прогон 36826523713); политика по `uriparse` записана в `dependency-quality.md`, зависимость не заменена |
| §10 Производительность | **исправлено** | стадия 1 — парсер (`analyze_file`, CLI), стадия 2 — проходы ссылок: `analyze_file_with_references` и `analyze_project_with_references` линейны по размеру файла, результат побайтно тот же (151 тыс. исходников против прежней реализации — 0 расхождений); см. §16.2 |
| §11 Тесты и fuzz | **частично** | два сквозных мутационных теста на стабильном Rust (`dartscope-parse` и `dartscope-index`, `robustness_mutations.rs`), которые группируют сбои по месту в коде и сводят их к минимальному воспроизведению; ими найдены три дефекта (§16.4). Не сделано: libFuzzer-цель для `analyze_file_with_references` (нужен nightly-job `ci.yml`, в этом цикле его не выполнить) |
| §12 Документация | **исправлено** | CHANGELOG, README (десять крейтов, `dartscope-lsp`, symlink-каталоги), `cli-contract.md`, `cli-input-limits.md`, `lint-cli.md`, `lint-rules.md`, `json-contracts.md`, `dartscope-library-plan.md`, `dependency-quality.md`, `AGENTS.md`, `.gitattributes` |
| §13 Пробелы | **открыты** | workspace-модель LSP; транзитивные Flutter-виджеты; `implements` не моделируется; приведение типа приёмника (`Foo().bar`) |

### 16.2 Производительность (Linux, release; стадия 1 — прогоны 36819670051 → 36824677154, стадия 2 — 36899634860 → 36904052941)

| Форма входа | До | После |
| --- | --- | --- |
| 16 000 классов в одном файле | 108 с | **0,28 с** |
| 80 000 функций | 7,7 с | **0,42 с** |
| 200 000 строк аргументов одного вызова | таймаут (> 100 с) | **1,5 с** |
| 20 000 классов в **одной строке** | 3,2 с (рост ×16,6 на ×4) | **0,06 с** |
| 20 000 методов с телами | рост ×≈16 | **0,19 с** (рост ×3,7 на ×4) |
| `pedometer_bindings_generated.dart`, 2,66 МБ | > 150 с (аудит), 4,1 с (промежуточно) | **0,28 с** |
| `analyze-project`, 484 файла `flutter/samples` | 223 с | **0,64 с** |

Причины, найденные и устранённые: таблица строк пересобиралась на каждый спан; каждый кандидат вызова сверялся со всеми
декларациями файла; сканирование членов и локальных переменных не останавливалось на конце тела и шло до конца файла
для каждой декларации с телом; колонка и глубина скобок считались от начала строки; оператор, занимающий тысячи строк,
пересканировался с каждой своей строки. Защита от регрессий — тесты, считающие просмотренные байты и строки, а не время.

**Проходы ссылок — исправлено (стадия 2).** После парсера оставались проходы ссылок (`lexical_reads`, `lexical_writes`,
`identifier_references`, `member_references`, `property_references`, `operator_references`, `lexical_regions`,
`lexical_bindings`): для каждого идентификатора они просматривали все привязки, ссылки, декларации и регионы файла, а
границу выражения или блока искали сканированием от токена. CLI их не вызывает (`analyze-file`/`analyze-project` не
затронуты); они работают в LSP и у библиотечных потребителей `analyze_*_with_references`. Время проходов после
`analyze_file`, один файл (до — прогон 36899634860, после — 36904052941, раннер был в полтора раза медленнее обычного):

| Форма файла | Размер | До | После |
| --- | --- | --- | --- |
| 4 000 классов, 16 001 декларация | 875 КБ | 27,5 с | **0,15 с** |
| 800 Flutter-виджетов (state, локальные переменные, замыкания, `for`) | 599 КБ | 11,6 с | **0,09 с** |
| один метод с 8 000 локальными переменными | 683 КБ | 51 с | **0,17 с** |
| одно выражение `Column(children: […])` с 8 000 дочерними | 598 КБ | 15,3 с | **0,06 с** |
| одно выражение из 16 000 слагаемых | 64 КБ | 1,65 с (после первой половины правки) | **0,006 с** |

Остальные измеренные формы — 16 000 стрелочных функций, 16 000 блоков с локальной одного имени, 8 000 импортов с
префиксом и вызовами, литерал на 16 000 элементов, 8 000 групп «замыкания, `for`, `try`» (1,35 МБ) — укладываются в
0,05–0,32 с. Время растёт в 1,9–2,4 раза при удвоении размера на всех формах (прежде — в 4–6 раз).

Что сделано. Проходы больше не обходят декларации, привязки и ссылки на каждом токене. На файл строятся `FileFacts`:
`DeclarationTables` (первая декларация по `symbol_id`, владелец, прямой член по имени, локальные переменные по
(родитель, имя), самый внутренний вызываемый по смещению) и `SourceStructure` (границы операторов, угловые скобки,
пары `()` и `{}`, конец выражения для любого начала); на каждый проход — `BindingIndex` (самая внутренняя видимая привязка
по имени и смещению с прежним правилом неоднозначности, интервалы деклараций и инициализаторов). Общие примитивы —
`IntervalSet`, `StabbingIndex`, `MinTree` в `interval_index.rs`; `StabbingIndex` точен и для перекрывающихся спанов битого
кода, чего не даёт индекс только по вложенности. Три копии `innermost_callable_symbol` и по две копии
`select_visible_binding` и других проверок в `lexical_reads`/`lexical_writes` стали одним кодом.

Как проверено. (1) Для каждой структуры есть тест эквивалентности со сканом, который она заменила, на случайных входах
(вложенные и перекрывающиеся интервалы, дубли `symbol_id`, несбалансированные скобки). (2) Дифференциальный прогон против
`ebf6b61` — отпечаток полного `Debug`-вывода `analyze_file_with_references` по 151 326 повреждённым исходникам из 21
затравки (9 реальных файлов мутационного теста и 12 сгенерированных форм): **0 расхождений, 0 паник**; первый прогон на
15 652 исходниках — тоже 0. (3) Тесты и clippy на трёх ОС. (4) Мутационный hunt (см. строку состояния выше).
Защиты от возврата к сканированию на токен — тесты эквивалентности, дифференциальный прогон (`fuzzing.md`) и
информационный тест `crates/dartscope-parse/tests/reference_pass_scaling.rs` (`#[ignore]`, печатает рост при
удвоении); проверки времени в тестах нет.

Что осталось. Квадратичным остался `analyze_file` (не проходы ссылок) на двух патологических формах: 4 000 вложенных
вызовов `f(g(g(…)))` (≈ 0,15 с) и 4 000 незакрытых `(` (≈ 0,1 с), потому что спаны вызовов вкладываются друг в друга.
Проходы ссылок на этих формах линейны (0,001–0,002 с). Предел LSP в 256 КиБ сохранён, но теперь он ограничивает работу
после каждой правки (около четверти секунды на МиБ в release), а не рост стоимости; поднять его можно отдельным решением.

### 16.3 Решения, которые стоит подтвердить

1. **Контракт инвентаря** (внутри v1, без смены major): значения `extends` у extension и `mixes_in` у mixin стали пустыми,
   `on`-типы переехали в `on_types`; enum-константы и top-level accessors теперь есть в выводе. Потребитель, читавший
   `extends` у extension как `on`-тип, должен перейти на `on_types` (`json-contracts.md`).
2. **`uri-graph`**: пустой/пробельный URI, `%2F`, выход за корень и `package:…/../…` теперь `invalid_uri` без `target_path`.
3. **Симлинки**: политика осталась fail-closed (ссылка наружу и симлинк-каталог — exit 3), кроме каталогов Flutter, которые
   пропускаются целиком.
4. **`lint`**: без `--config` он по-прежнему ничего не запускает; Error-диагностика проекта по-прежнему прерывает `lint` (exit 6).
5. **LSP**: документы свыше 256 КиБ не участвуют в навигации.

### 16.4 Что нашли мутационные тесты

Детерминированное «повреждение» реалистичных исходников (удаление, дублирование и вставка фрагментов, обрезка,
лишние разделители и кавычки, BOM, CR/CRLF, не-ASCII) и случайные последовательности правок индекса сразу нашли то,
чего не нашли ни ручные пробы аудита, ни 96 тыс. запусков узких fuzz-целей:

1. **Паника в `lexical_regions::scan::is_control_header`**: не-ASCII символ перед `(` в коде, который не является
   корректным Dart (`é(`), — срез слова по байту вне границы символа. Для LSP это падение сервера на опечатке.
2. **Паника в `find_top_level_keyword`**: тот же класс, `for(é)` — срез в цикле, который посещает каждый байт.
3. **Устаревшие данные в инкрементальном индексе**: после правки тела класса ниже его первой строки, переименования члена
   на имя той же длины или смены `extends` зависимые файлы сохраняли прежние разрешения (§5.2 был исправлен лишь для
   однострочных примеров).

После исправлений кампания из 324 тыс. повреждённых исходников и 18 тыс. последовательностей правок (~72 тыс. обновлений
индекса) не нашла ни паник, ни расхождений, ни неверных спанов. Методика и параметры — `docs/development/fuzzing.md`.
После переписывания проходов ссылок (§16.2) кампания повторена: 324 тыс. повреждённых исходников — ни паник, ни неверных
спанов (прогоны занимают по 8 секунд вместо минут), а дифференциальный прогон против прежней реализации на 151 тыс.
исходников не нашёл ни одного расхождения.

### 16.5 Отложено

Разбиение `incremental.rs`; libFuzzer-цель; исключения по суффиксу и сегментные префиксы в `lint`;
транзитивные Flutter-виджеты; компактный/потоковый вывод и детерминированный `root` в CLI; workspace-модель LSP
(сканирование корня, `pubspec.yaml`, отмена запросов); замена `uriparse`; branch protection на `main`; прогон `cargo audit`,
nightly-задачи fuzz и benchmark-отчёта на финальном дереве (запустятся на pull request).
