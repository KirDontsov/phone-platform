# Фаза13 — backend implementation и проверки

Дата: 2026-11-12. Внешние модели, ASR, Телфин и интеграции в этой проверке не вызывались. Фаза12 остаётся приостановлена.

## Исследование причины

На работающем стенде worker healthy. Без чтения секретов/данных клиентов проверены агрегаты:542 transcribe failed с demo_provider_not_allowed; configured demo ASR не предназначен для реальных звонков. Прежний POST reprocess всегда создавал wrapper и возвращал job_id, а detail не раскрывал дочернее задание и причину. Авторизация Codex помощника не является конфигурацией ASR.

## Реализация

- `phone-api/src/processing.rs`: tenant-scoped bulk projection transcript/analysis/processing, HashMap выбора jobs (без O(calls*jobs)), готовность по документам текущей расшифровки, отдельная partial оценка, safe error allowlist. Сохранённые spoofed analysis_state/transcript_state/score не принимаются за доказательство готовности. Старые анализы остаются в detail.
- `crates/persistence/src/lib.rs::processing_jobs`: все active jobs компании и последний historical processing job на call/kind; нет лимита200, payload из read model исключён. Очередь учитывает ready/running/scheduled, ahead только в компании, ETA=null. Чужие счётчики, payload, lease_owner и произвольные legacy ошибки не возвращаются.
- `phone-api/src/calls.rs`: detail.processing.queue/providers; reprocess под call row lock, active wrapper/child dedup до preflight; новые200/повтор202, prerequisite422 без enqueue. Transcribe требует только запись+ASR; analyze — текст+analysis+rubric; all сохраняет повторную полную цепочку. Provider readiness проверяет конфигурацию локально, не заявляет live работоспособность.
- Callback first-episode numeric callback_age_minutes/callback_attempts и analysis_state/transcript_state в AST. Возраст пересчитывается относительно текущего времени, правила работают до date/search; successful follow-up вне периода закрывает эпизод. `/callbacks` принимает тот же AST, что list/dashboard. Uncertain не включается в доказанные неперезвоненные.
- Dashboard analyzed включает реальные analyzed/partial, средняя оценка берётся из актуального результата, null не считается нулём.
- `phone-worker/src/pipeline.rs`: ready флаг/состояние записи согласованы с API; при действительном запуске/успехе очищаются устаревшие processing_reason/retry_pending; prompt_version=call-quality-v2-checklist. Worker сохраняет собственные проверки конфигурации и изоляцию.
- Исправление независимого review: failed новый wrapper не скрывается успешным старым child; succeeded wrapper не маскирует активный/ошибочный child.
- Исправление независимого review экспорта: новый amoCRM/Sheets export выбирает analysis для последнего transcript, score из выбранного результата/null. Существующий delivery сохраняет pinned analysis_id, включая null; отсутствующий зафиксированный анализ даёт pinned_analysis_missing без подмены результата.

## Проверки реализации

- `TEST_DATABASE_URL=<isolated> cargo test --locked -p phone-api --test processing_contract --test recording_availability -- --ignored --test-threads=1`:2/2 PASS. Новый HTTP/PG processing test проверяет текущесть, partial, queued/running/retry/failed/completed, устаревший wrapper, чужой tenant/same UUID, >200 newer jobs, safe redaction/errors, preflight/no-enqueue, concurrent requests200+202 same job, повтор после disconnect, missing recording/transcript/analysis/rubric, динамический callback AST и паритет трёх endpoints, закрытие follow-up вне datefilter. Recording availability regression повторён.
- `TEST_DATABASE_URL=<isolated> cargo test --locked -p phone-worker --test pipeline -- --test-threads=1`:7/7 PASS на отдельной настоящей PostgreSQL. Новый export test проверяет obsolete/current transcript, mismatch persisted99 vs analysis61, фактические Sheets columns, pinned old/null/missing; остальные6 сохраняют pipeline/manual overrides/budget/purge/uncertain delivery.
- `cargo test --locked -p domain relative_callback_and_analysis_filters_keep_numeric_types_and_unknown_separate`:1 PASS, типы relative AST и исключение uncertain/attempted.
- `cargo check --locked -p phone-api`:PASS; rustfmt owned files выполнен.

Основная новая API regression сначала воспроизведена RED на исходном поведении: persisted analyzed при отсутствии анализа текущего transcript. Дополнительно4 RED mutations в удалённой после проверки копии исходников: убрать preflight → ожидаемый422 не получен; отключить dedup → concurrency assert падает; вернуть низкий priority failed wrapper → скрытая ошибка; убрать matching export transcript → экспорт obsolete результата. Все четыре завершились101 по assertion, не по compile/setup. Domain-only новый тест отдельно RED не проверялся; дополнительные guard cases в одном интеграционном тесте отдельно не мутировались.

Логи приватны: data/eval/phase13-processing-red.log, phase13-processing-green.log, phase13-worker-pipeline-green.log, phase13-backend-red-summary.json и соответствующие *-red.log. Две отдельные тестовые БД phone_platform_processing_test/phone_platform_processing_worker_test использовались вместо рабочей; исходные подключения, настройки, данные и авторизации не менялись.

Независимый verifier/полный frontend UAT и внешняя приёмка ASR/analysis/amoCRM/Sheets фиксируются root отдельно. Эти результаты не доказывают качество реального распознавания/анализа или доставку во внешние аккаунты.
