# Локальный пилот и границы приёмки

Дата: 2026-11-11. Реализованы Rust API/worker/shared crates, Vue SPA, локальная PostgreSQL/RabbitMQ/Mailpit инфраструктура и optional OpenCode runner. Проверки ниже относятся к синтетическим данным и реальным локальным HTTP/БД/браузеру. Реальный пилот на аккаунтах Телфина, ASR/LLM, amoCRM и Google не выполнялся. Пользователь проведёт финальный UAT по [сценариям](uat.md); его результат пока не получен.

## Подтверждённые проверки

| Слой | Фактический результат | Граница evidence |
| --- | --- | --- |
| Телфин CLI | 56 offline tests PASS | Synthetic transport, настоящие FS/SQLite; live requests=0 |
| Domain/contracts | 15 unit PASS:13 domain+2 crypto/access | AST, callback/coverage/DST, strict scoring/evidence, tags≤30 unique, authenticated encryption |
| Persistence | 1 real PostgreSQL integration PASS на отдельной testDB, включая последний uncertain-retry regression | Composite tenant FK/versioning/outbox/rollback/leases/fencing/tombstones; ignored default test не считается запуском PG |
| API | 2 real Actix/PG HTTP tests PASS | Auth/reset one-use/logout/CSRF/origin/tenant/RBAC/manual lock/stale409/redaction, dashboard callback regression |
| Worker | 29 distinct worker tests PASS: lib 6+providers 14+HTTP 5+pipeline PG 4; combined Rust 47 distinct PASS | Provider/loopback contracts, delivery uncertainty, import coverage/versioning и budgets; внешние вызовы заменены synthetic responses |
| Frontend | 20 unit/7 files PASS, vue-tsc+Vite build PASS | API cancellation/late tenant responses, AST/forms, versions/roles, nullable cost, history_start verification UX |
| Browser | Полный Docker E2E13/13 PASS, zero skips,44.1s; после access audit/DNS исправлений affected2/2 PASS,14.8s |13 distinct сценариев;2affected — повтор существующих, не15 новых |
| OpenCode runner | 2 real Docker/HTTP smoke PASS на 1.18.10 |401 без пароля, deny-all permissions/tools, MCP0/plugins0, session create/delete, readonly/nonroot/tmpfs/no hostports/mounts; model prompts=0 |
| Backup tools | 14 tests PASS; реальный disposable PG ledger replay и Docker media-plan PASS | Full Compose create→isolated restore PASS:22 ready media references/5 removed docs/ledger 1/no resurrection/shared retained 1 |

Точные команды и mapping26 планов: [execution research delta](../.planning/research/EXECUTION-DELTA.md). Frontend screenshots и real E2E provenance: [frontend verification](../.planning/reviews/FRONTEND-VERIFICATION.md). Нельзя складывать повторные/целевые прогоны как новые независимые tests; skipped tests не учитываются как PASS.

Demo audio — искусственный WAV-сигнал: плеер получил duration 32/readyState 4/error=null. Это подтверждает media delivery/плеер, но не распознавание речи или соответствие текста звуку. Mobile390 проверен на dashboard/settings/detail/transcript/analysis, горизонтальный overflow устранён.

Clean-volume Compose acceptance также PASS:5 checks — fresh migrations/readiness, nginx auth/two companies, private Range206 media, API→job→worker→transcript/analysis, company switch/foreign404. Random project и volumes удалены; `data/eval/compose-smoke.json`.

Offline quality harness: [ASR](asr-evaluation.md) и [AI](analysis-evaluation.md);10 synthetic tests PASS,2 CLI unknown-reference smoke PASS. Reports `data/eval/asr-harness-smoke.json` и `analysis-harness-smoke.json` содержат acceptance_passed=null, reference_samples=0. Реальное качество не заявляется.

## HTTP query load profile

Команда `python3 scripts/load-test.py` создала две временные компании, добавила5000 synthetic calls в одну и canary в другую, выполнила login/switch/проверки и удалила обе компании после logout. Доступ к реальным аккаунтам не использовался; jobs ASR/LLM/import не создавались.

| Endpoint | Запросов | p50, мс | p95, мс |
| --- | ---: | ---: | ---: |
| calls list |8|468.82|616.44|
| calls с AND AST |8|356.86|537.77|
| dashboard |8|546.94|752.08|
| callbacks |8|431.56|517.41|

Concurrency 4,32 измеренных запроса, wall 4.026 s,7.95 requests/s. Есть отдельный прогрев. При8 sample p95 совпадает с максимумом выборки; это небольшой локальный профиль, не production SLO. Технический threshold5 s выбран для smoke и не является согласованным SLA.

Проверены точные5000 calls/1250 answered/2500 missed/1250 overdue,1250 resolved episodes с одной неуспешной попыткой перед успешным звонком, AST result1250, согласованность dashboard, отсутствие duplicated pagination, foreign callID404, unknown average_score=null. Машинный отчёт: `data/eval/load-report.json` (локальный приватный файл без IDs/телефонов/секретов).

API пока загружает историю компании в память и фильтрует Rust evaluator. Calls/day, peak concurrency, длительность реальных записей, backfill/queue drain, ASR/AI latency и cost/min не измерены. Для существенно большей истории нужны отдельный профиль и SQL read models; этот результат не доказывает ёмкость для произвольной компании.

## Независимый review и устранённые дефекты

- API: полная история callbacks до display filter; validation merged callback policy; extension сохраняется при role-only PATCH; новый manual sync не дедуплицируется с прежним done; detail/list используют согласованный derived contact_kind.
- Worker: coverage расширяется только завершённым перекрывающим scan; import сохраняет optimistic version и сериализуется по call; budget reservation опирается на fencing, а не обнуляемые при ручном retry attempts. Review повтор и итоговый freeze suite отдельно фиксируются.
- Domain/persistence: automatic tags имеют тот же max 30/unique контракт, что manual UI; uncertain delivery можно явно повторить для reconciliation, marker/payload сохраняются. Worker не выполняет слепой повтор POST/append при неизвестном исходе.
- Frontend: mobile min-content overflow, unknown cost→0, immutable role version, rubric_version, stale optimistic form revision и явная непроверенная нижняя граница истории.
- Backup: exact checksum manifest, запрет special archive members, restart после partial stop, ledger после даты backup, recursive tenant-scoped purge и один media helper вместо контейнера на файл.

## Recovery evidence

На отдельной PostgreSQL БД replay удалил5 связанных документов, включая grandchildren и call с тем же provider ID; cascade удалил jobs/outbox, другая компания с таким provider ID сохранилась. Ledger компании, отсутствующей в старом снимке, сохранился; повторный replay идемпотентен, последующий экспорт сохранил все записи. БД удалена после проверки. Отдельный Docker volume подтвердил удаление и наличие файлов с пробелами/литеральными спецсимволами; volume удалён.

Полный snapshot/create→isolated restore с настоящим Compose media volume PASS: baseline 2 организации/4 users/241 documents/9 jobs,22 ready media references,1 актуальная ledger entry,5 post-backup documents удалены, shared media retained 1, sourceDB не изменена, восстановленные sessions/grants отозваны. Отчёты: `data/eval/backup-create.log`, `backup-verify.json`, `deletion-ledger-export.json`. RPO/RTO не согласованы и не измерены. Последний deletion ledger и ключ шифрования должны храниться независимо от ротации backup; старый снимок не знает о последующих удалениях. [Backup/restore runbook](backup-restore.md).

## Открытые gates

- Телфин: права/ownership конкретной АТС, pagination boundaries/depth/late updates, доступность записи и независимая сверка полноты. M0 остаётся открытым; all_available без verified lower bound остаётся bounded probe сcoveragefalse.
- ASR/AI: разрешённые русские записи/диаризация/роли, held-out rubric/evidence/abstention quality, выбранная модель/Codex auth, реальные usage/цена/квоты. Offline eval-transcripts/eval-analysis harness создан; реального annotated corpus нет.
- amoCRM/Google: OAuth/scopes/refresh/revocation и фактическая доставка/идемпотентность непосредственно в аккаунтах пользователя. Внешние CRM/Sheets копии не удаляются автоматически.
- Реализационные отклонения: webhook/subscription lifecycle не создан; polling 5 min+manual reconcile работает как текущий механизм. AST SQL compiler/cached read models не созданы. Retention — единый срок компании, не отдельные сроки artifact types; backup at-rest encryption обеспечивает оператор.
- Финальный Docker/nginx/browser/backup повтор выполнен. Пользовательский UAT, production deployment target, SMTP/TLS/alerts и release decision остаются открытыми.

Ни один внешний gate или пользовательское подтверждение не объявляется выполненным по демонстрационным данным. Для финального теста подготовлены [UAT](uat.md), [ограничения](known-limitations.md), [выпуск](release-runbook.md) и [rollback](rollback-runbook.md).

## Последний affected closure

Cross-phase review выявил OPS-01 gap: auth invite/member операции не оставляли audit. Добавлены атомарные `invitation.created/revoked` и `membership.joined/updated/removed`; targeted real API/PG2/2 PASS проверил tenant/actor/subject, previous role и отсутствие события при отказе доступа. На новых Docker-образах invitation+real worker reprocess2/2 PASS без skips,14.8s; audit events связаны invitation ID. PII/токены не выводились. Итого остаётся13 distinct browser сценариев плюс2affected повторных проверки; полный13 suite после этих двух узких исправлений не повторялся.

nginx runtime DNS отдельно проверен реальной сменой backend Docker IP при неизменном frontend: URI `/api` и query сохранены, `nginx -t` PASS. Тестовые контейнеры/сеть удалены. Это закрывает воспроизведённый rebuild/recreate сценарий, не заявляет внешнюю публикацию.

Итоговые Python80 tests PASS:56 CLI+14backup+10evaluation. Rust47 distinct PASS; access-audit assertions добавлены в существующие2 API tests. Исходники зафиксированы: `e789168` (platform), `7af83b7` (ops/evaluation), `3563c4f` (access audit/nginx DNS). Документация/финальный независимый38-requirement report обновляются отдельно; external/live и пользовательский UAT остаются открытыми.
