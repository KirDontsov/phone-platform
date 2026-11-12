# Roadmap: Phone Platform v1

Created/updated: 2026-11-11. Ниже исходные цели фаз; фактическая реализация и evidence записаны в phase SUMMARY/VERIFICATION, открытые acceptance gates — в UAT.

## Реализация для финального теста

Все 10 фаз исполнены на уровне кода и локальных проверок, 26 планов имеют summaries/mapping. Веб-стенд: http://localhost:18081; запуск `scripts/dev.sh up`. Rust47 / frontend20 / Docker E2E13 / CLI56 PASS; clean-volume Compose, runner isolation и actual backup/restore проверены. [Фактический отчёт](../docs/pilot-report.md), [состояние](STATE.md), [UAT](../docs/uat.md).

Пользовательский UAT и live-приёмка Телфин/ASR/LLM/CRM/Sheets остаются Pending; поэтому milestone M0/M3 и все фазы не помечаются полностью принятыми. Последнее указание пользователя разрешило выполнять разработку следующих фаз до закрытия M0. Ключевые отклонения реализации: polling вместо webhook lifecycle, AST в Rust вместо SQL, общий retention компании — [решения](../docs/architecture-decisions.md).

## v1.0 — Milestones

- **M0 — API evidence:** фаза 1, реальная выгрузка и проверенный контракт Телфина.
- **M1 — Рабочий журнал:** фазы 2–4, вход, АТС, синхронизация, записи и ручная разметка.
- **M2 — Аналитический MVP:** фазы 5–7, расшифровка, AI-оценка и контроль необработанных обращений.
- **M3 — Полный v1:** фазы 8–10, CRM/Таблицы, аналитика, эксплуатация и пилот.

Во всех фазах обязателен цикл research → plan → plan-verifier → revision-loop → execute → verifier → UAT. Будущие исследования — стартовые гипотезы; перед execute обновлять документацию и контракты по результатам предыдущих фаз.

## Phases

### Phase 1: Telphin API spike — вся доступная история

**Goal:** Получить реальные звонки до разработки большой платформы и доказать корректность контракта.
**Depends on:** Nothing. Для live-проверки нужны локально настроенные права API.
**Requirements:** TEL-01, TEL-02, TEL-03
**Success Criteria:** OAuth работает без утечки токенов; все страницы контрольного периода выгружены; учтены missed/no-recording/internal; определены call/cdr IDs, границы времени и история доступности; повторный запуск стабилен; отчёт не заявляет полноту при ошибке страницы.
**Plans:** 01-01 контракт и CLI; 01-02 сверка полноты и failure fixtures.

### Phase 2: Основа платформы, авторизация и компании

**Goal:** Платформа сразу для нескольких независимых компаний с изолированными данными, безопасным входом и доступом сотрудников.
**Depends on:** Phase 1
**Requirements:** PLAT-01, AUTH-01, AUTH-02, AUTH-03
**Success Criteria:** Compose стартует с чистой БД; миграции повторяемы; вход/logout/reset/invite работают; tenant isolation проверена на двух организациях, включая подключения/секреты, настройки, медиа и задачи; выбор компании проверяет членство, переключение очищает состояние клиента и игнорирует ответы/события предыдущей компании; роли и сопоставление добавочных доступны владельцу.
**Plans:** 02-01 Rust scaffold; 02-02 Vue scaffold; 02-03 DB/contracts/jobs foundation и полный Compose smoke; 02-04 auth/session; 02-05 membership/roles/invites.

### Phase 3: Подключение Телфина и надёжная синхронизация

**Goal:** Полная доступная история и дальнейшие звонки поступают автоматически, без дублей и потерь при сбоях.
**Depends on:** Phase 2
**Requirements:** CONN-01, SYNC-01, SYNC-02, SYNC-03, SYNC-04
**Success Criteria:** Владелец управляет подключением и периодом; сохраняются checkpoints/плечи/raw source; polling с overlap догоняет потерянные события; поздние записи обновляются; 429/expiry/restart не дают ложного success; есть DLQ/retry и прогресс.
**Plans:** 03-01 connection/historical ingestion; 03-02 incremental/events/reconciliation; 03-03 resilience/outbox.

### Phase 4: Журнал, карточка, записи и ручная разметка

**Goal:** Пользователь уже работает с реальными звонками без зависимости от AI.
**Depends on:** Phase 3
**Requirements:** CALL-01, CALL-02, CALL-03, TAG-01, TAG-02, DASH-01
**Success Criteria:** Журнал/карточка/базовый дашборд количества, направлений и технических результатов согласованы; пропущенные не выдаются за необработанные; плеер перематывается; expired/missing/pending recording понятны; статусы и теги редактируются; override сохраняется; доступ к файлу другой компании невозможен.
**Plans:** 04-01 calls API/list/dashboard; 04-02 card/audio; 04-03 taxonomy/manual audit.

### Phase 5: Аудиопайплайн и расшифровка

**Goal:** По записи доступна воспроизводимая расшифровка с сегментами и участниками.
**Depends on:** Phase 4
**Requirements:** ASR-01, ASR-02
**Success Criteria:** ASR-адаптер проверен на русских записях; большие файлы режутся без потери границ; роли не угадываются как факт; сегмент ведёт к позиции в аудио; нет записи/тишина/ошибка отделены от готовой расшифровки.
**Plans:** 05-01 storage/ASR/long audio; 05-02 transcript UI/evaluation/reprocess.

### Phase 6: AI-анализ, шкалы качества и авторазметка

**Goal:** Диалог получает объяснимую оценку и разметку через сменяемый модельный backend.
**Depends on:** Phase 5
**Requirements:** AI-01, AI-02, AI-03, AI-04, AI-05
**Success Criteria:** Проверен Docker runner OpenCode и Codex smoke либо честно зафиксирован unavailable; схема результата строгая; цитаты ссылаются на сегменты; rubric/version/usage видны; нет hallucinated IDs; ручной override остаётся; первичность учитывает неполную историю.
**Plans:** 06-01 provider spike/runner; 06-02 schemas/rubric/scoring; 06-03 classification/revisions/UI/evals.

### Phase 7: Конструктор фильтров и контроль перезвонов

**Goal:** Руководитель задаёт воспроизводимые выборки и видит зависшие обращения.
**Depends on:** Phase 6
**Requirements:** FLT-01, FLT-02, FLT-03, FLT-04
**Success Criteria:** AST фильтров один для UI/API/агрегатов; SQL параметризован; AND/OR/NOT и сохранение работают; missed после ответа другим сотрудником исключается; attempt/connected различаются; таймер без новых звонков и late import меняют выборку правильно; счётчики необработанных и просроченных обращений на дашборде раскрываются в ту же выборку.
**Plans:** 07-01 typed filter builder; 07-02 callback episodes/time rules.

### Phase 8: amoCRM и Google Таблицы

**Goal:** Связать разговор с процессом продаж и выгрузкой результатов.
**Depends on:** Phase 7
**Requirements:** CRM-01, CRM-02, SHEET-01, INT-01
**Success Criteria:** OAuth lifecycle проверен; многозначное совпадение не выбирает сделку случайно; публикация идемпотентна; Sheets хранит стабильный ключ; revocation/quota/retry видны; сбой интеграции не блокирует журнал и AI.
**Plans:** 08-01 amoCRM; 08-02 Sheets and delivery reliability.

### Phase 9: Аналитический дашборд, аудит и управление обработкой

**Goal:** Руководитель видит качество и потери, администратор — работоспособность и затраты.
**Depends on:** Phase 8
**Requirements:** DASH-02, OPS-01, OPS-02
**Success Criteria:** Агрегаты раскрываются в идентичную выборку; unknown/not-applicable не считаются нулевым качеством; аудит имеет автора и версии; видны queue lag/provider errors/budget; reprocess сохраняет версии и overrides.
**Plans:** 09-01 analytical dashboard; 09-02 audit/operations/budget controls.

### Phase 10: Хранение, пилот, проверка и выпуск

**Goal:** Доказать готовность v1 на реальных звонках и воспроизводимое восстановление.
**Depends on:** Phase 9
**Requirements:** OPS-03, REL-01
**Success Criteria:** Полный E2E путь пройден; privacy/retention удаляют связанные данные и не допускают их воскрешения при retry; backup восстановлен в изолированную среду; согласованы качество и нагрузочный профиль; пробелы live-проверок не замаскированы fixtures; есть runbook и release/rollback checklist.
**Plans:** 10-01 lifecycle/restore; 10-02 pilot/evals/load/release.

## Progress

| Phase | Реализовано планов | Локальная проверка / приёмка |
|-------|--------------------|-----------------------------|
| 1 | 2/2 | CLI offline PASS; live pending |
| 2 | 5/5 | PASS; user UAT pending |
| 3 | 3/3 | PASS; live Телфин pending |
| 4 | 3/3 | PASS; user UAT pending |
| 5 | 2/2 | PASS; live ASR pending |
| 6 | 3/3 | PASS; live quality pending |
| 7 | 2/2 | PASS; user UAT pending |
| 8 | 2/2 | PASS; live CRM/Sheets pending |
| 9 | 2/2 | PASS; user UAT pending |
| 10 | 2/2 | Local restore/evals PASS; live pilot pending |
| 11 | 3/3 | API/runner/browser PASS; live/user UAT pending |

Итого: 29 планов реализации (26 baseline +3 assistant). Реализация не означает закрытую пользовательскую/внешнюю приёмку. План основания платформы разделён после независимой проверки объёма: Rust scaffold, Vue scaffold и схема данных/надёжные задания выполняются отдельно. Сроки и бюджет уточняются после phase 1 и замера объёма/длительности звонков; календарные обещания без этих данных не даются.

### Phase 11: Configuration Assistant

**Goal:** Владелец через чат с Codex в отдельном Docker-контейнере подключает Телфин и настраивает статусы, теги, фильтры, шкалы и правила перезвона через scoped MCP.
**Requirements:** CHAT-01, CHAT-02, CHAT-03, CODEX-01, CODEX-02, MCP-01, SETUP-01
**Depends on:** Phases 2–7, 9–10
**Plans:** 11-01 scoped chat API/shared settings; 11-02 Docker Codex/MCP/auth; 11-03 UI/integration.

Research →3 plans → independent plan-check →revision1/3 PASS → execute. Локальная реализация есть у всех трёх планов; independent verifier и UAT фиксируются отдельно. Внешняя авторизация/модель/Телфин — pending, synthetic ответы не выдаются за live inference.

### Phase 12: Telphin Credential Onboarding

**Goal:** Подключить Телфин из обычных логина/пароля АТС без обязательных APIполей; billingотдельно, APIadvanced.
**Requirements:** ONBOARD-01, ONBOARD-02, ONBOARD-03
**Depends on:** Phases3,11
**Plans:** 12-01 fixed portal +scoped durableprovision;12-02 ordinaryaccessUX+9toolintegration.

Research: officialbootstrap +anonymousloginHTML; behindauthmarkup/livepermissionspending. Контракт docs/contracts/telphin-onboarding-v1.md. Дваплана/5tasks structurevalid. Independentcheck/revision gatepending; execute не объявленcompleted.

### Phase 13: Call Workspace Acceptance

**Goal:** Довести четыре подтверждённых сценария заказчика до понятного рабочего экрана и прозрачного ASR→analysis pipeline.
**Depends on:** Phases4–9,11. Фаза12 приостановлена и не является зависимостью.
**Requirements:** CALL-01/03, TAG-01, ASR-01, AI-02, FLT-03/04, DASH-02, OPS-01, CRM-01, SHEET-01.
**Plan:** 13-01 processing/relative callbacks, Dashboard/shared filters/sidebar, checklist/Sheets workflow.
Research → независимый PLAN-CHECK PASS0/3 → execute → independent implementation review/fixes → verifier → automated UAT. Локальная реализация не закрывает живое ASR/модельное качество/CRM/Google и финальную пользовательскую приёмку. Все результаты: phases/13-call-workspace-acceptance.

### Phase 14: Local Analyzer / Docker Tunnel / OpenCode ASR

**Goal:** удалённый siteworker→DockerSSHrelay→localOpenCodeconnector, one-commandstartup идве выбранныеASRмодели.
**Depends on:** Phases3,5,6,13. Phase12остаётсяpaused.
**Requirements:** ASR-01/02,AI-01,PLAT-01,OPS-01.
**Plan:**14-01: hardenedconnector+exactcatalog; portablelocalstartup/remoterelay; worker/APItenantSTT; settings/UAT.
Research→independentPLAN-CHECK PASS0/3→execute→independentreview/fixes→verifier→automatedUAT. Implementation+localchecksесть; SSHrealhost/pairing/OpenRouterqualityиhumanUATpending. GSDplanstructurevalid4tasks; подробностиphases/14-local-analyzer-opencode.

### Phase 15: Production / GitHub / Real Analyzer Pairing

**Goal:** HTTPS phone.bondscan.ru на разрешённом сервере, сохранённые Phone DB/media/Codexauth, работающий remote→local analyzer и опубликованная master.
**Depends on:** Phases10,11,13,14. Phase12paused.
**Plan:**15-01 source-only amd64 release; protected first transfer; own nginx/TLS and real tunnel; independent verifier/UAT/GitHub.
Research→independentPLAN-CHECK PASS0/3→execute in progress. Known demo passwords меняются, новые находятся только в локальном0600 secrets/production-access.json; encryption key/Codex generation сохраняются. Другие приложения и shared cache не меняются. Финальная ownerUAT и реальная ASR quality отдельно от TLS/tunnel health.
