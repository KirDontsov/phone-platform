# Phase 20 — Telphin queue / cached recording implementation

Дата: 2026-11-13. Gate: `20-PLAN-CHECK.md` PASS0/3 до изменений. Область исполнителя B: persistence/worker и synthetic actual-PG tests; UI/OAuth выполняются отдельно root.

## Изменения

- `crates/persistence/src/lib.rs`: preclaim исключает sync/recording только своей компании при будущем Telphin cooldown. Проверяются JSON number type, целочисленный формат и граница i64; JSON text не приводится к числу. Остальные компании/kinds продолжают работу. Приоритет: due purge/retention, затем due sync, затем прежний порядок остальных jobs. Сохранены due, expired-running lease, max-attempts, SKIP LOCKED и fencing guards.
- `phone-worker/src/providers.rs`: ProviderError различает сохранённый connection backoff и вновь наблюдённый rate-limit. HTTP/network контракты и коды ошибок прежние.
- `phone-worker/src/pipeline.rs`: чтение cached backoff не продлевает общий deadline; настоящий provider-rate-limit сохраняет максимум текущего и нового deadline. Готовность записи определяется одинаково с API (`ready=true` или `state=ready`), до доступа к подключению. Ошибка не понижает готовый recording и не меняет его path. Успешное скачивание удаляет старый last_error.
- Новые tests: три actual-PG persistence regression, один actual-PG worker internal regression, один actual-PG pipeline/media regression.

Не менялись production jobs, attempts, checkpoints, ручной retry, подключения или credentials. В Телфин не отправлялись новые запросы. Production build/deploy не выполнялся исполнителем B.

## Actual PostgreSQL и GREEN

Изолированный локальный Docker `phone-phase20-pg`, PostgreSQL 16-alpine, tmpfs 256 MB, только loopback18520, отдельная synthetic БД `phone_phase20`. Production не используется для fixtures. Тестовый URL содержит исключительно synthetic пароль, не пользовательские секреты.

Команды:

```sh
TEST_DATABASE_URL=postgres://phase20:synthetic-only@127.0.0.1:18520/phone_phase20 cargo test -p persistence --test postgres -- --ignored --test-threads=1
TEST_DATABASE_URL=postgres://phase20:synthetic-only@127.0.0.1:18520/phone_phase20 cargo test -p phone-worker --lib --test pipeline --test providers --test model_http -- --test-threads=1
```

Исполнитель: три новых persistence tests PASS; worker lib20, pipeline8, modelHTTP13, providers16 PASS с actual-PG, без skips. Независимый reviewer дополнительно сообщил persistence4/4 PASS, включая прежний migration/tenant/leases/purge regression; окончательный независимый результат фиксируется verifier.

Проверены: attempts/fencing неизменны при пропуске cooldown; другая компания проходит; истёкший lease откладывается во время cooldown и восстанавливается после с новым fencing; старый владелец не может finish; due/max-attempt guards сохраняются; purge/retention/sync опережают recording backlog; malformed/huge/missing JSON не падает; cooldown не блокирует другой kind. Worker actual-PG проверяет неизменность connection version/data при cached recording backoff, невозможность сократить текущий deadline новым коротким 429, продление настоящим более длинным 429 и сохранность ready/path/version. Pipeline проверяет локальный непустой synthetic файл и `state=ready` без provider config.

## RED

До fix на исходных исходниках действительно упали:

1. `phase20_cooldown_skips_jobs_without_attempts_and_preserves_tenant_and_expired_lease`: забиралась job заблокированной компании вместо доступной другой.
2. `phase20_control_and_history_are_not_starved_by_recording_backlog`: выбирался recording вместо control/history.
3. `phase20_ready_state_recording_never_contacts_provider_or_loses_cached_path`: API-playable state-only ready запись падала из-за отсутствия provider config.
4. Internal `phase20_cached_backoff_does_not_renew_deadline_or_downgrade_ready_media`: в отдельной baseline sourcecopy cached failure переписал connection metadata (version3 вместо2).

Malformed-JSON compatibility test на исходном коде проходил; его не выдаём за RED. Все отдельные assertions большого internal test и строка успешной очистки last_error по отдельности RED не проверялись.

Во время четвёртого RED временная sourcecopy ошибочно использовала общий Cargo target, из-за чего независимый прогон один раз загрузил baseline artifact и упал. Общие исходники не откатывались. Ошибка устранена: RED процесс завершён, sourcecopy и отдельная red DB удалены, очищены только собственные package artifacts `cargo clean -p phone-worker -p persistence`, затем актуальные исходники root полностью пересобраны и worker20+8+13+16 повторно PASS. Независимый reviewer предупреждён и повторяет GREEN последовательно. Дальнейших RED workspace/shared-target операций нет.

## Открытые проверки

Реальное восстановление original production sync и смена очереди после следующего cooldown проверяются root после отдельного deployment. Число готовых recording/file metadata до реализации подтверждено в `20-TELPHIN-RESEARCH.md`; реализация не объявляет недоказанное удаление аудио и не обещает полный импорт, пока coverage не подтверждён. Пользовательская UAT остаётся отдельной.

## Urgent revision 1 — explicit retention consent

После первоначальной публикации root выявил: новый приоритет очереди ускорил существующую retention с неявным default90days, которая удаляла исторические данные по started_at. По сообщению root до остановки worker 120 звонков purged, 24 528 только soft-deleted; самостоятельно production-состояние после этого исполнитель B не менял/не перепроверял. Исходное исследование выше описывает состояние **до** этого deployment. Восстановление данных и возможность вернуть 120 purged — отдельная работа root/recovery agent, не закрытая настоящим guard.

После independent PLAN-CHECK revision1 PASS реализовано только в worker/pipeline/tests:

- Retention требует **текущий** `organization.data.retention_enabled == true`; missing/null/string/false не включают удаление. Учитывается актуальное колонковое retention_days компании, а не старый job payload. До orphan cleanup проверяется enable, перед маркировкой calls в guarded transaction политика читается повторно.
- Каждый purge с dedup `retention:` перед unlink повторно читает живую политику и полноту/возраст call. При disabled либо продлённом сроке не удаляет запись/детей/файл. Уже purged, tombstoned и неполные/redacted calls не оживляются.
- Снимается только доказанная retention soft-deletion: текущая retention-origin job, целый call с provider_id/валидным started_at, нет tombstone, нет audit action/event `call.deleted`, нет любого non-retention purge job. Аннотации/прочие данные сохранены. Если ручное удаление доказано, soft-delete не снимается.
- Будущие retention soft-deletes получают origin/job markers. Ручной purge выполняется по прежней ветке. Explicit enabled/expired retention действительно удаляет intended файлы/детей и создаёт tombstone.

Новые три actual-PG теста покрывают defaultoff, строгий boolean enable, currentdays вместо stale payload, выключение после enqueue, продление срока после enqueue, manual audit в обоих форматах, manual purge job, purged/partial/tombstone guards, положительное enabled retention и отдельное ручное удаление при выключенной retention.

Два ключевых теста запущены **до** fix на текущих исходниках и действительно упали: defaultoff всё равно ставил deleted; queued retention purge после отключения всё равно unlink файл. В R1 нет sourcecopy/общего-target RED. Затем актуальный код: worker lib20 + pipeline11 + modelHTTP13 + providers16 **PASS**, без actual-PG skips, `git diff --check` PASS. Не все ветки/каждый assertion отдельно проверены RED.

Synthetic PostgreSQL `phone-phase20-pg` пересоздан для R1 и передан root/reviewer для последовательных API/independent checks; пока не удалять до их завершения. Во время тестов не запускать конкурентно другой global claim fixture в этой же synthetic БД. Production mutations, resets и публикация исполнителем B не выполнялись. Guarded bulk reconcile и финальный независимый verifier остаются задачей root.

Root подтвердил независимый GREEN61/61. Изолированный контейнер phone-phase20-pg удалён после проверок; tmpfs не сохраняется, чужие контейнеры/образы не очищались.
