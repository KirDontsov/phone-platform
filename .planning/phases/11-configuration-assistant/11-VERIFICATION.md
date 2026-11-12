---
phase: "11-configuration-assistant"
status: local_verified_external_pending
verified_at: "2026-11-12"
requirements-completed: []
human_uat: pending
external_acceptance: pending
---

# Фаза 11 — независимая проверка реализации

Локальная реализация трёх планов проверена. Встроенный чат, отдельный Codex runtime, настоящий SDK MCP bridge и настройки через API существуют в коде и исполняются в локальном контуре. На момент исходной проверки реальная авторизация Codex/inference ещё не выполнялись. После пользовательского входа исправлен authenticated-startup: настоящий ответ и MCP чтение теперь подтверждены, см. [11-STARTUP-FIX](11-STARTUP-FIX.md). Live Телфин, качество настройки/оценки и итоговый пользовательский UAT остаются открытыми. Фаза не объявляется полностью принятой.

Проверяющий не писал core assistant API, Node runner, frontend или backup.py. Он написал и исполнил независимые black-box API/PG tests и targeted backup tests, просмотрел фактические исходники/контракт и отчёты остальных исполнителей. Авторство тестов не выдаётся за независимость от тестовой методики; backend assertions не ослаблялись под реализацию. Проверка качества плана отдельно: [11-PLAN-CHECK](11-PLAN-CHECK.md), PASS после одной доработки из трёх допустимых.

## Исполненное evidence

| Область | Фактическая проверка и источник | Практический предел |
| --- | --- | --- |
| API/DB | Проверяющий: шесть различных ignored acceptance tests явно запущены с isolated PostgreSQL и настоящими Actix/reqwest listeners: **6/6 PASS**, 29.27s. После последних исправлений финальный targeted scope **1/1 PASS**, 2.94s, включая101короткий статус и точный failed-receipt replay | Private runner — явный loopback wire fixture, не модель. Product fallback отсутствует |
| Конкурентная власть | Настоящие отдельные PG connections и barriers, ожидание блокировок через pg_stat_activity: cancel/tool в обоих порядках, удаление сессии, downgrade роли и смена компании. После подтверждённого revoke изменение не применяется; более раннее committed действие сохраняется | Это проверка линейной точки записи локального API, не распределённая exactly-once гарантия внешнего Телфина |
| Durable lifecycle | Partial сохраняется при молчащем upstream; duplicate submission возвращает тот же run, changed body409; unknown/malformed/oversize/EOF fail closed, maintenance закрывает просроченные runs. Порядок user→assistant, model/usage и terminal events проверены | Partial не означает успешное выполнение. Cancel не откатывает уже committed настройки |
| MCP receipts/CAS | Реальный gateway изменяет taxonomy/filter/rubric/callback policy, подключение и sync job в изолированной БД. Hash replay, stale revision/rowversion, scope и strict nested schema проверены. Ошибки сохраняют receipt и расходуют durable budget24; replay прежней ошибки сохраняется после исчерпания лимита | Sync только поставлен в очередь; внешний импорт в этом тесте не исполняется |
| Credentials/auth | Sentinel отсутствует в GET/history/events/audit/model history. Capability в DB hash-only. Consumed/expired credential ciphertext удаляется. Device/APIkey protocol fixture проверяет staged completion, исходного owner/session, поколения, logout и поздний callback | APIkey/device ответы fixture синтетические; настоящий account approval/entitlement не проверен |
| Backup | Проверяющий: **17/17 PASS** `test_backup_tools.py`, без skips с `PHONE_BACKUP_PG_TESTS=1`:15unit+2actualPG. Реальный SQL deletion-ledger replay удаляет только tenant subtree, не воскрешает chat/credentials; восстановление отзывает sessions/grants/capabilities/auth, удаляет временный ciphertext и сохраняет permanent provider config | Обычный backup исключает auth volume. После restore требуется повторный вход Codex |
| Настоящее восстановление | Root: actual create→isolated restore, отчёты `data/eval/phase11/backup-create.json` и `backup-restore.json`;22ready media, no-resurrection и authority revoke/purge PASS | Исходная БД не подменялась; это не production disaster cutover/RPO/RTO |
| Runner | Исполнитель runner: **15/15 Node tests** локально и те же15/15 в hardened Docker (4.86s). Включены exact approved+generation, isolation/TTL/logout, EOF/forbidden RPC, cancel/disconnect cleanup и SDK schema/allowlist/budget | Повтор внутри Docker не считается ещё15 различными тестами |
| Реальный executor/SDK | Настоящие pinned Codex0.167.2 и MCP SDK1.32.2, production capability config плюс локальный Responses fixture: разрешённый MCP get_workspace_configuration дошёл до API; forced exec_command получил отказ, marker не создан.3loopback requests,0external/paid/auth calls | Это настоящий executor negative probe с фиктивным upstream, не paid inference и не model quality |
| Docker boundary | `data/eval/assistant-runner-smoke.json`: healthy, UID10001, read-only root, cap-drop ALL, no-new-privileges, resource limits, нет hostport/bind/repository/DB/socket/media; только private auth volume, secrets DB/broker/encryption отсутствуют | Docker имеет сетевой transport к фиксированным API/auth/model endpoints; модель не получает generic HTTP tool |
| Frontend | Исполнитель UI: **43/43 unit PASS**, vue-tsc/Vite build PASS, шесть затронутых baseline browser scenarios PASS. Actual Docker noauth assistant4/4 PASS без skips4.8s | Unit и browser не доказывают полезность модели |
| Configured browser | Отдельный test-only API18083/PG/loopback runner18084 + Vite18085: **4/4 PASS**, без skips8.4s на свежем бинаре: owner/history/secureform/tenant, mobile, reviewer, настоящий MCP gateway→созданный тег→reload и hold→cancel | `status.model=test-wire-fixture`, ответ явно синтетический; external requests0 |
| Main API↔runner | Прочитан `data/eval/assistant-main-wire-smoke.json`: actual API18080→private Node8091, runner ready/version0.167.2/authfalse, message409 без создания message/run, random capability403, temporary chat deleted404, session logout | Без входа владельца и модельного запроса; настройки не изменялись |

Команда API: `cargo test -p phone-api --test assistant -- --ignored --test-threads=1 --nocapture --skip support_assistant_browser_fixture`, с `TEST_DATABASE_URL` только из приватной локальной настройки на отдельную `phone_platform_assistant_test`. Helper `support_assistant_browser_fixture` — тестовый listener, не седьмой acceptance test и не production code. Он запускается отдельно с `ASSISTANT_BROWSER_FIXTURE=1`; fixed test accounts существуют только в отдельной тестовой БД.

Команда backup: `PHONE_BACKUP_PG_TESTS=1 python3 -m unittest discover -s scripts/tests -p test_backup_tools.py -v`. Без флага два actualPG tests будут skipped; такой запуск не является их evidence. Node: `npm --prefix infra/assistant-runner test`; Docker: `docker compose --env-file .env.platform -f infra/compose.yml exec -T assistant-runner npm test` и `npm run smoke`.

Frontend final evidence: main Docker4/4 и configured fixture4/4 — **пять различных case definitions на двух targets**, не восемь новых различных сценариев; повторы не увеличивают этот счётчик. Последний fixture visual report `data/eval/phase11/assistant-fixture-visual.json` прочитан проверяющим: ошибок0, mobile viewport/document390, composer bottom793, sequence user→assistant, все5events completed. PNG desktop/mobile визуально просмотрел исполнитель frontend. После browser acceptance тестовые API/runner остановлены graceful shutdown, отдельная browser DB удалена; продуктовый сервис не заменялся fixture.

## Реестр семи требований

| Требование | Реализовано и проверено локально | Незакрытая часть |
| --- | --- | --- |
| CHAT-01 | Owner FAB/page, own-user/org history, rename/delete, cleanup при switch, desktop/mobile/reviewer browser checks | Финальная оценка удобства пользователем |
| CHAT-02 | Durable partial/run/events, idempotent requests, failure/recovery/cancel, chronology, receipts/audit и реальные lock races | Пользовательский UAT на своей задаче |
| CHAT-03 | Отдельная secure card/ref, encryption/TTL/physical cleanup, user/org/conversation scope, sentinel privacy и restore | Сохранение разрешённых реальных Телфин credentials владельцем |
| CODEX-01 | Отдельный pinned Docker/native executor, capability denial, SDK stdio bridge, no host/Bondscan auth, process cleanup; analysis MCP0 не ослаблен | Риск будущего обновления CLI требует повторения pinned negative probe |
| CODEX-02 | UI device/APIkey, staged original-session completion, generation/expiry/logout/cancel, явный noauth без fake model answer | Настоящее account approval, policy/entitlement и первый inference |
| MCP-01 | Ровно8typed tools, fixed gateway, live original authority, CAS, atomic receipt/audit, failure budgets, schema и cancellation races | Телфин HTTP конкретного аккаунта и его реальные побочные эффекты |
| SETUP-01 | Инструменты connection/test/sync/taxonomy/filter/rubric/callback, UI результатов; wire test реально создаёт тег в PG | Успешное выполнение естественной задачи настоящим Codex и импорт конкретной АТС |

## Обнаруженные ошибки и исправления

Во время execute/verifier выявлены и устранены: auth-generation TOCTOU между status и create-run; отсутствие повторной generation проверки при partial/completion; nondeterministic chronology одинаковых timestamps; потеря model/usage; ошибки schema вне durable budget/receipt; stale rowversion в no-op lookup; silent partial до следующего upstream chunk; lock inversion и FK deadlock; недостаточно строгие nested rubric/calendar поля; неограниченный общий размер workspace; неверные uncertain statuses после completed. Последний source review дополнительно обнаружил silent `.take(100)` в списках workspace: заменён явным413, regression использует101короткий статус, не достигающий bytecap.

Shared source review и авторство остальных finding: [11-RUNNER-API-REVIEW](../../reviews/11-RUNNER-API-REVIEW.md). Финальный targeted scope проверяет byte/count caps, nested rejection, metadata, completed statuses и прежние scope/replay/CAS/privacy assertions. Обнаруженные сбои не скрываются под последующими PASS.

## Открытые проверки

Нет локального критичного дефекта в проверенном scope, требующего остановить пользовательский UAT. Это не отсутствие возможных дефектов вне покрытых путей. Не были выполнены: реальная device approval/APIkey entitlement; платная модель и корректность её плана/текста; live test connection/import Телфина; полнота истории конкретной АТС; production deployment/нагрузка чата; human acceptance. Инструкции и ожидаемые результаты — [11-UAT](11-UAT.md) и [docs/uat](../../../docs/uat.md).

Полный baseline workspace/browser набор не повторялся при каждом локальном исправлении фазы11. Повторялись затронутые tests; старые результаты фаз1–10 остаются baseline evidence и не выдаются за свежую внешнюю приёмку.

## Пользовательский startup regression

После входа пользователя обнаружены несовместимое startup notification и невидимые для known-model MCP инструменты; исправлены без ослабления boundary. Прежний native probe обходил orchestrator/использовал fallback model metadata; теперь добавлены fullorchestrator и knownproductionmodelcatalog cases. Реальный gpt-6.1-sol отвечает и читает настройки через get_workspace_configuration, контрольные чаты удалены. Affected API2/2 и frontend43/43 PASS. Полный followup evidence и ограничения: [11-STARTUP-FIX](11-STARTUP-FIX.md).
