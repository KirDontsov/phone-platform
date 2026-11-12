---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: Phone Platform
status: phase_12_paused_by_user
stopped_at: User paused phase12 and clarified they need to locate Telphin API credentials; drafts preserved, not deployed
last_updated: "2026-11-12"
progress:
  total_phases: 12
  implemented_phases: 11
  locally_verified_phases: 11
  accepted_phases: 0
  completed_phases: 0
  total_plans: 31
  implemented_plans: 29
  completed_plans: 1
---

# Project State

## Текущее состояние — фаза 11

Пользователь запросил встроенный чат как в Bondscan: Codex в Docker через MCP сам подключает Телфин и настраивает статусы, теги, фильтры, шкалу и правила перезвона. Доступы передаются защищённой карточкой внутри чата; модель получает encrypted credential_ref. Новый assistant-runner отделён от анализатора записей, у которого MCP остаётся выключенным.

Контракт: docs/contracts/assistant-v1.md. Research и три плана: phases/11-configuration-assistant. Root — автор плана/API; fix_cli_review — независимый plan-checker; domain_persistence — исследование и реализация runner; frontend_platform — исследование и реализация UI. Модели наследуются. Независимый PLAN-CHECK PASS после доработки1/3; реализация прошла execute gate. API/runner/UI реализованы и проверены локально; финальный пользовательский тест и живые аккаунты остаются отдельной приёмкой.

Проверены Codex npm 0.167.2 и MCP SDK 1.32.2. Host/Bondscan auth не читалась, запросов к моделям не было. Собственная авторизация Codex будет в UI: device code/API key. История ограничена owner/user/org; capability на каждый tool привязан к исходной сессии, run и TTL. Мутации, receipts и audit атомарны, есть CAS/cancel/revoke/deadline. Разрешены восемь типизированных инструментов настройки; нет IAM, retention, export, произвольных HTTP/SQL, shell, репозитория или Docker socket.

Фаза11: Codex0.167.2/MCP SDK1.32.2, отдельный hardened Docker runner,8typed tools, owner chat drawer/fullpage/mobile, защищённые карточки Телфина, собственный model login, durable history/run/events/cancel. Общие settings services/CAS/audit; credentials physical GC/restore revocation. Нет product fake response fallback.

Проверки phase11: API6/6 на настоящих Actix/PostgreSQL/loopbackHTTP (scope,originalsession,generation,receipts,CAS,races,partial/EOF,GC); затронутые HTTPplatform2/2; Node/Docker15/15 включая actual pinned Codex+SDK positive/forcedforbiddennegative с локальным Responses fixture. Frontend43/43 unit/build; assistant E2E mainDocker4/4 +isolatedwirefixture4/4, это5distinctcases на2targets, baselineaffected6distinctPASS. Screenshots1440/390, overflow0, JSerrors0. Python81offlinePASS +actualPGbackup2/2; отдельный backup17/17включаяэти2. Полный create→isolatedrestore повторPASS,22ready media, auth/capabilities revoked, ciphertextpurged, noresurrection. Машинные отчёты приватны в data/eval/phase11 и data/eval/assistant-*.json.

Независимые review findings исправлены и проверены: authgeneration TOCTOU, порядок сообщений, metadata, failed tool budget/replay, DB lock inversion, nested schema parity, false uncertain status наcompleted, неполный settings snapshot. Research/plan-check/summary/verification/UAT — phases/11-configuration-assistant. Первая planrevision1/3 PASS; implementationverification отдельно.

Стенд http://localhost:18081 запущен. Проверка владельцем: открыть «Помощник» →войти в Codex кодом/APIkey →«Добавить доступы» для Телфина →обычная задача текстом. Настоящий model/device/Telphinlive и humanUAT pending; платных внешних model requests0. База v1 и её внешние проверки сохраняются.

## База v1 до фазы 11

Веб-платформа «Сигнал» реализована и запущена в независимом Docker project phone-platform: http://localhost:18081. Учебный владелец owner@demo.local / PhoneDemo!2026, две изолированные компании. API/worker/client/PostgreSQL/RabbitMQ/Mailpit и необязательный OpenCode runner healthy. Повторный запуск: `scripts/dev.sh up`.

Пользователь поручил автономно выполнить все фазы и оставить ему финальный тест. Разработка не останавливалась на live gate M0. Предыдущее указание сначала работать без живого Телфина соблюдено: реальные Телфин/ASR/LLM/CRM/Google не вызывались. Credentials и данные Bondscan не использовались.

Все 26 планов имеют реализацию или явный mapping к фактическим файлам и описанные архитектурные отклонения. Для 10 фаз созданы SUMMARY/VERIFICATION/UAT; implementation baseline готов, внешний и пользовательский acceptance не объявляются complete. `completed_phases=0` отражает отсутствие финального подтверждения, а не отсутствие кода. `completed_plans=1` сохраняет строгий статус полностью закрытого CLI-плана; частично внешние acceptance criteria остальных планов не подменены локальными тестами.

## Проверки

- Rust: 47 различных PASS, включая отдельные PostgreSQL/Actix и loopback HTTP checks; workspace compile без warnings. После последнего purge fix повторён затронутый pipeline, не весь workspace без необходимости.
- Frontend: 20 unit PASS, vue-tsc/Vite build PASS; полный Docker/nginx E2E 13/13 без skips, 44.1s. После access audit/DNS исправлений affected2/2 повтор PASS14.8s, actual audit events и смена Docker API IP без рестарта client проверены. Desktop1440/mobile390, overflow0, JS errors0, audio32s/ready4/errornull.
- Telphin CLI: 56 offline PASS; live requests0.
- Python: 80/80 PASS = CLI56 + backup14 + evaluation10.
- Backup: safety tests и настоящие create/isolated restore PASS; 22 ready media, 5 документов удалённого после backup звонка не восстановлены, ledger1, shared audio сохранено. Исходная БД не подменялась.
- Clean Compose: новые случайные БД/тома, миграции, auth2companies, private audio Range206, очередь→текст→анализ и foreign call404 PASS; тестовый project удалён.
- OpenCode1.18.10: реальный Docker deny-all/auth/session smoke PASS, model prompts0. Quality harness ASR/analysis реализован, без reference выдаёт acceptance=null.
- Load: 5000 synthetic calls/concurrency4, p95 списка616ms, dashboard752ms; это небольшой локальный профиль, не production SLA.

Актуальные детали и воспроизводимые команды: docs/pilot-report.md, .planning/research/EXECUTION-DELTA.md, .planning/reviews/IMPLEMENTATION-REVIEW.md, .planning/reviews/FRONTEND-VERIFICATION.md, .planning/reviews/FINAL-VERIFICATION.md. Машинные отчёты в приватном Git-ignored data/eval.

## GSD и решения

Research → 26 plans → независимый PLAN-CHECK → 2 baseline revisions → execute → implementation review/fix/regression → verifier → automated UAT. Финальный human UAT оставлен пользователю по его указанию. PLAN-CHECK не считается проверкой готового продукта. Commits: e789168 платформа; 7af83b7 backup/eval/smoke; 3563c4f access audit/DNS; 0656c50 CLI fixes. Последний runtime manifest: data/eval/release-manifest.json.

Стек Bondscan сохранён, сервис и данные независимы. Composite tenant FK + membership session guard, durable DB jobs/fencing + Rabbit outbox, private media volume, encrypted provider configs, immutable transcript/rubric/analysis, manual locks, strict AI evidence. Конкретные решения: docs/architecture-decisions.md, docs/contracts/platform-v1.md.

## Осталось для внешней приёмки

Финальный пользовательский тест по docs/uat.md и настройка разрешённых аккаунтов. Нужно сверить полноту/границу истории Телфина, качество русского ASR/диаризации/оценок на reference corpus, фактические OAuth/scopes/доставку CRM/Sheets и тарифы моделей. Публичный deployment target не задан и не публиковался. Вопросы пользователю не задаются: открытые параметры описаны как настраиваемые предположения.

Отклонения: polling5min с overlap вместо webhook lifecycle; фильтры/агрегаты пока Rust in-memory вместо SQL read models; единый retention компании. Никакое неподтверждённое внешнее качество не называется PASS. Следующее действие — пользовательский финальный тест готового стенда, затем исправление конкретных замечаний.

## Повторное code review — 2026-11-12

Исправлены: передача названий тегов/статусов модели; сохранение unknown-исхода доставки через ошибку reconciliation/credentials и retry; явное отключение ASR/analysis без demo fallback. Подробнее reviews/IMPLEMENTATION-REVIEW.md, раздел2026-11-12. Свежие worker33 (включая настоящую отдельную PostgreSQL), core15, Python80, frontend20 PASS; новые4 regressions, совокупный набор Rust51 (API2/persistence1 — прежние результаты, не новый повтор). API/worker Docker пересобраны; полный browser13 — прежний baseline, текущий targeted smoke проверяет новые версии анализа и сохранение ручных полей. Короткая пользовательская проверка: docs/uat.md → «Быстрая проверка». Внешние аккаунты не использовались.

## Исправление пользовательского UAT: startup Codex — 2026-11-12

Пользователь вошёл в Codex, но любой prompt падал. Причина: authenticated startup присылает informational remoteControl/status/changed, отсутствовавший в строгой event allowlist. Исправлены известное notification, first-error preservation и safe stage/code diagnostics, API/UI error classes, duplicate connecting status и misleading uncertain для обычных status. Запрет unknown tools/events/RPC сохранён.

Свежие Node19/19 и affected API2/2 PASS, frontend43/43 PASS; native positive/forbiddennegative теперь проходит полный createRunnerHTTP→TurnService, не обходит orchestrator. Реальный запрос через уже подключённый ChatGPT account завершён: gpt-6.1-sol ответил «готово». Проверочный чат удалён, настройки и авторизация сохранены. Прежние заявления model requests0 относятся к baseline до этой live проверки; Телфин/анализ речи/CRM всё ещё не проверялись. Подробности phases/11-configuration-assistant/11-STARTUP-FIX.md.

Дополнительный live MCP дефект также исправлен: модель с tool-search catalog получала настройки как deferred без доступного механизма поиска. Ровно8Phone Platform tools теперь direct-only (`omit_tools_from=[deferred,code_mode]`); встроенные OS/code-mode инструменты по-прежнему выключены. Настоящий get_workspace_configuration через gpt-6.1-sol завершилсяcompleted, revision0 сохранена, контрольный чат удалён. Docker/API/client пересобраны/healthy, пользовательский вход сохранён.

Startup followup закрыт локально: Node20/20 вhost/Docker, knownproductionmodelcatalog +fullorchestrator, forcedOS/JS/edit denied, affectedAPI2/2, frontend43/43. Пользователь затем уточнил новый UXgap: есть логины кабинета и АТС, а неAPIAppIDSecret. Начинается фаза12 — получениеAPIприложения через защищённый PBX onboarding. Текущий API-only способ остаётся advanced, новый пользовательский ввод не должен требовать неизвестных емуAPIполей.

Пользователь остановил переделку («Нет, погоди») и уточнил: не знает, где получить APIдоступ. Фаза12 приостановлена; plancheck PASS не означает реализацию. Написанные draftform/adapter файлы сохраняются, не опубликованы и не считаютсяпроверенными. Dockerостаётсяна рабочем startupfix d013faf (Codexreply+MCPreadPASS,20Node/nativechecks). Следующееобщение — инструкцияпоофициальному app/register, беззапросасекретов.

## Client ID — узкий пользовательский фикс

Commit ef3f00e: App ID/App Secret достаточны в APIформе чата и настройках; отсутствующий/пустой Client ID определяется только по authenticated /user/. DeployedAPI/worker/clienthealthy;32worker+4API+45frontend+actualbrowser1PASS. Основные missing-IDregressions REDподтверждены, securitynegativeRED отдельнонепроверяли. Настоящие ключи Телфина пользователем ещёнепроверены. Детали phases/11-configuration-assistant/11-CLIENT-ID-FIX.md.

Phase12implementationdraft сохранён в отдельном Gitstash `ddddf25f4ef384b0a02359042efa4bee159a1f09` («Paused phase12 implementation; API Client ID fix remains in main»), вmainоставлен только работающий API-key путь. Plans/researchсохраняются; phase12неисполнятьбезвозобновленияпользователем. Stashнеудалятьдоуспешногоresume/review.

## Записи: лимит API и ожидание плеера

После liveимпорта ready391real(+17demo),pending126; provider429→retryна13:00UTC. ReadyfileпроверенHTTP206иmutedbrowserplayback. ИсправленAudioPlayer: pending/ratelimitedнеоткрываетmedia иобъясняетожидание, readyпоpollingвключаетсяавтоматически, готовыйфайлвыбираетсябезсбросавоспроизведения. Frontend51/51,5newbehaviourregressionsRED,actualbrowserready+pendingPASS. Clientdeployed. Подробностиreviews/RECORDING-AVAILABILITY.md. Фаза12остаётсянапaузе; лимитproviderобходомне снимается.

## Доступность записи в списке

CALL-01/CALL-03: вычисляемый recording_state, колонка «Запись», мобильный badge и быстрый фильтр по6 состояниям. «Можно слушать» означает завершённую загрузку; has_recording/stage не подменяют доступность. Один tenant-scoped bulk read без N+1. API/PG1 интеграционный PASS с17 случаями и RED без фикса, frontend56/56 и5 новых RED, domain14/14, cargo check/build PASS. API/client deployed healthy; реальный browser ready-filter507total/25rows, mobile390 без overflow, JS0. Детали reviews/RECORDING-LIST-AVAILABILITY.md. Финальный human UAT остаётся пользователю; фаза12 не возобновлялась.
