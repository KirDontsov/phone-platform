# Phase20 revision1 independent implementation review

Дата: 2026-11-13. Объём: API/settings/operations, Vue organization settings, worker retention/purge, scoped bulk reconcile. Source120 recovery CLI проверяется отдельно после готовности.

Статус: **ожидается окончательный current-tree GREEN / batch regression**. Production worker остановлен, R1 ещё не опубликован; настоящий repair/source запросы reviewer не выполняет.

## API / UI

Блокирующих замечаний в исходниках не найдено. `retention_enabled` сохраняется в organization.data только как JSON boolean, stricttrue является opt-in; отсутствие флага не включает автоудаление. PATCH использует прежний owner/CSRF/settings-change transaction и row lock, чужие поля/компании не меняются. Operations purge отказывает при defaultoff. Callback/manual settings unaffected.

UI checkbox по умолчанию unchecked, manager control disabled, срок недоступен пока false. Текст явно предупреждает, что при включении удаляется и загруженная история с аудио/текстами/анализами, возраст от даты звонка. Независимый scoped UI **2/2 PASS**. Operator сообщает actualPG API2/fullUI100/type/build PASS и meaningfulRED; reviewer просмотрел source, independent API повтор ожидается в общем current source run.

## Worker

Live policy читается из собственной organizations row, strict boolean true; job.payload не может переопределить актуальный срок. Disabled retention handler выходит до выбора historical calls и удаления. Основная транзакция повторно проверяет политику; новые soft deletes имеют deletion_origin/retention_job_id.

Retention-origin purge проверяет политику перед unlink. Если policy выключена/срок увеличен — целая call может быть восстановлена только при наличии валидного provider_id/started_at, без purged/tombstone/manual-delete audit (оба event/action) и manual purge job. Partial/tombstoned/purged остаются скрытыми, manual purge остаётся исполнимым. Сохранены tenant, lease/fencing и path/shared-media guards.

Ограничение: изменение политики после уже пройденного guard в выполняющемся filesystem unlink не отменяет начавшееся удаление; проверка действует перед началом job unlink. Для текущей recovery worker остановлен, активных удалений нет.

Исполнитель сообщает workerlib20/pipeline11/modelHTTP13/providers16 GREEN и RED2 до fix. Первая независимая попытка на одновременно изменяемом worktree остановилась на compile error нового in-progress recovery.rs (Read/Write by_ref ambiguity), не на R1 retention; recovery исполнителю сообщено. Требуется повтор после source freeze.

## Bulk reconcile

`scripts/reconcile-retention.py` приватно читает0600 manifest с лимитом32MiB/100k, нормализует UUID, без PHI output. PostgreSQL transaction SERIALIZABLE, собственная organization row FOR UPDATE и disabled check; dry-run выполняет ROLLBACK. Каждый target перепроверяется live через exact org/call/purge/retention-parent/dedup, queued/retry либо expired running, no active lease; manual audit/job/tombstone/already-purged исключены. UPDATE сохраняет остальные JSON поля, повышает version; cancellation повышает fencing, не сбрасывает attempts. Receipt только aggregate, commit error не выдаётся за success. Повтор idempotent по deleted/jobstate predicates.

Запрошенное замечание: вместо только JSON type=string требуются непустой provider_id и безопасная проверка валидной даты, чтобы malformed partial call не была undeleted repair-скриптом; worker helper уже строже. Нужны actualPG dryrun/apply/idempotence/foreign/manual/tombstone/partial regressions до apply.

Замечание закрыто: добавлены btrim(provider_id), ISO prefix и безопасный Postgres16 pg_input_is_valid(timestamp with time zone). Независимый actualPG `test_retention_reconcile.py` **1/1 PASS** на отдельной синтетической БД: dry-run rollback, guarded apply, repeat idempotent, strict enabled block; manual audit/job/tombstone/purged/foreign/malformed date/empty provider исключены. Первая попытка соединения с уже удалённой root fixture api2 не состоялась, затем тест действительно выполнен на сохранившейся изолированной phone_phase20.

**Bulk reconcile scope review PASS; разрешён guarded APPLY существующего точного private manifest в рамках порученного исправления, пока worker STOPPED и backup/snapshot подтверждён root.** Это разрешение reviewer gate для данного soft-delete repair, не для source120 восстановления/возрождения ручных удалений. Последующий receipt и readonly сверка обязательны. Current whole-tree/R1 release gate остаётся pending до GREEN/build/verification.

Source recovery120 и итоговая production verification остаются отдельными gate; первичное удаление120 не считается восстановленным по одному bulk undelete24528.
