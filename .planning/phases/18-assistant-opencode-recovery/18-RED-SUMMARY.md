# Phase18 — независимая проверка RED

Проверяющий: opencode_settings_ui. Настоящий исходный HTTP/PG тест автора прошёл GREEN: `data/eval/phase18-api-local-models-pg.log`, 1/1,93.74s.

Затем создана отдельная временная копия только текущих Rust sources, Cargo manifests/lock и migrations под ignored `data/eval/phase18-red-rust-*`. `.env`, secrets, customer data/media не копировались. Рабочий source tree не менялся. Использовались отдельный PostgreSQL fixture на127.0.0.1:35437 и синтетические новые организации/пользователи с UUID на каждый прогон. Ни один production endpoint или upstream model не вызывался.

1. Вызов `validate_model_key_transition` в `save_connection_in_db` ограничен `kind == "asr"` вместо `matches!(kind, "asr" | "analysis")`; все остальные guards/org injection остались прежними. Настоящий HTTP запрос переключения analysis на внешний адрес без нового ключа получил **200 вместо ожидаемого400**. Тест упал на `internal pairing cannot follow an external destination`, exit101;45.5s вместе с компиляцией. Лог: `data/eval/phase18-red-analysis-guard.log`.
2. Первая мутация восстановлена в копии. Только список обязательных полей local preflight заменён на старый безусловный `model/base_url/api_key`. Настоящий GET карточки вернул `processing.providers.asr.ready=false` вместо ожидаемогоtrue при сохранённом keyless local ASR. Тест упал именно на этом assert, exit101;52.1s вместе с компиляцией. Лог: `data/eval/phase18-red-local-preflight.log`.

Оба failed-run fixture очищены по собственному synthetic UUID/email: организации и пользователи. Результат: `data/eval/phase18-red-rust-result.json`, у обеих `expected_red=true`, `fixture_cleaned=true`, `worktree_mutated=false`. Временная source-копия удалена.

Использовался общий Cargo target cache с штатными блокировками. Его artifacts не являются release: следующий обычный cargo запуск сам пересобирает актуальные sources; нельзя запускать mutation test binary напрямую как GREEN. Deployment строится из проверенного source revision, а не из этих локальных artifacts.

Отдельно пять новых UI tests проверены RED→GREEN на исходном UI, затем полный frontend95/95+typecheck/build PASS — см.18-UI-SUMMARY.md. Эти RED доказательства относятся к указанным HTTP/preflight/UI регрессиям, не к каждому отдельному assert или внешней OAuth/ASR quality.
