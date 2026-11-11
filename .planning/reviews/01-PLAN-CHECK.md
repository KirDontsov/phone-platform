---
status: issues_found
reviewed_at: "2026-11-11"
reviewer_role: gsd-plan-checker
reviewer_agent: gsd_final_verifier
review_scope: "10 phases, 24 plans, root requirements/architecture and phase context/research/validation"
blockers: 1
warnings: 1
implementation_verified: false
---

# Независимая проверка планов

Проверен финальный пакет после предварительной revision 1. Результат: **ISSUES FOUND — 1 BLOCKER, 1 WARNING**. Реализация отсутствует; этот отчёт проверяет исполнимость и покрытие планов, а не готовность продукта.

## Охват

Прочитаны задачи всех 24 PLAN.md, требования и архитектура, контекст всех 10 фаз, research и validation strategy. Дополнительно использован установленный `gsd-tools.cjs` для разбора frontmatter и проверки структуры каждого плана.

| Фаза | Планы | Требования | Покрытие | Результат |
|------|-------|------------|----------|-----------|
| 1 — Telphin API spike | 01-01, 01-02 | TEL-01…03 | 3/3 | PASS: вся доступная история, calls/CDR, coverage и отдельная live-сверка |
| 2 — Платформа и доступ | 02-01…03 | PLAT-01, AUTH-01…03 | 4/4 | ISSUES: объём 02-01 и browser verification приглашения |
| 3 — Телфин sync | 03-01…03 | CONN-01, SYNC-01…04 | 5/5 | PASS: checkpoint, overlap, inbox/outbox, retry/DLQ/fencing |
| 4 — Рабочий журнал | 04-01…03 | CALL-01…03, TAG-01…02, DASH-01 | 6/6 | PASS: карточка/медиа/ручная разметка, базовые технические метрики |
| 5 — Расшифровка | 05-01, 05-02 | ASR-01…02 | 2/2 | PASS: versions, offsets, role uncertainty, failure и quality cases |
| 6 — AI-анализ | 06-01…03 | AI-01…05 | 5/5 | PASS: provider spike, изоляция, schema/evidence/rubric, manual locks, history-aware classification |
| 7 — Фильтры и callback | 07-01, 07-02 | FLT-01…04 | 4/4 | PASS: typed AST и deterministic episodes/clock/coverage |
| 8 — amoCRM/Sheets | 08-01, 08-02 | CRM-01…02, SHEET-01, INT-01 | 4/4 | PASS: OAuth, ambiguity, stable keys и uncertain delivery без слепого повторного POST |
| 9 — Аналитика/операции | 09-01, 09-02 | DASH-02, OPS-01…02 | 3/3 | PASS: canonical filters/as_of, знаменатели, audit/redaction, budget/reprocess |
| 10 — Lifecycle и пилот | 10-01, 10-02 | OPS-03, REL-01 | 2/2 | PASS: purge/tombstones, isolated restore, live pilot/UAT/release evidence |

Суммарно **38/38** требований присутствуют в owning phase и в frontmatter соответствующих планов; непокрытых или неизвестных requirement IDs нет. Содержательные задачи покрывают заявленные требования, включая ошибки и неопределённость. Публичный SaaS, биллинг и неподтверждённое «лидирование» не добавлены в scope.

## Статическая проверка

- 24/24 PLAN.md проходят `verify plan-structure` без errors/warnings.
- 48 implementation tasks имеют `<verify><automated>…`; два checkpoint остаются явно ручными и имеют `autonomous: false` у плана.
- Ссылки зависимостей существуют, waves согласованы; циклов не обнаружено.
- У всех 10 фаз есть RESEARCH с Validation Architecture и NN-VALIDATION.md. Все validation documents честно помечены draft, `nyquist_compliant: false`, `wave_0_complete: false`.
- Существующие относительные Markdown-ссылки внутри `.planning` разрешаются. Будущие артефакты описаны как будущие, а не как доказательство наличия реализации.

Статическая корректность формата не снимает два содержательных замечания ниже.

## Issues

```yaml
issues:
  - id: FINAL-01
    dimension: scope_sanity
    severity: blocker
    plan: "02-01"
    file: ".planning/phases/02-platform-auth/02-01-PLAN.md:7"
    finding: "В одном плане 26 files_modified: полный Rust/Vue/Docker scaffold, integration smoke, tenant/access schema, shared contracts и transactional jobs/outbox foundation. Два крупных задания скрывают несколько самостоятельных результатов; превышен GSD blocker threshold 15+ файлов, а новая job foundation увеличила объём уже после первоначального разделения."
    suggested_fix: "Разделить phase 2 на четыре последовательных плана. 02-01: scaffold 14 путей — root Cargo.toml, API Cargo/main/Dockerfile, worker Cargo/main/Dockerfile, client package/main/router/Dockerfile, compose.yml, .env.example, Playwright config. До foundation не добавлять зависимости на отсутствующие shared crates и не заявлять полную migration readiness. Новый 02-02: shared crates (6 путей), access migration up/down, migration test, jobs repository, smoke-compose.sh и compose.test.yml, плюс wiring Cargo.toml; здесь выполнить полный build/up --wait/migrate/readiness/dependencies smoke. PLAT-01 завершается только после 02-02. Старые auth/invites перенести в 02-03/02-04, сохранить jobs/outbox до phase 3; обновить counts, dependencies, summary references, research/validation и roadmap. Не уменьшать files_modified искусственно."
  - id: FINAL-02
    dimension: verification_quality
    severity: warning
    plan: "02-03 (после разделения 02-04)"
    file: ".planning/phases/02-platform-auth/02-03-PLAN.md:86"
    finding: "Action теперь явно обещает browser recipient flow и отрицательные E2E приглашения, но task verification содержит только type-check и npm test -- members. Не запланирована явная Playwright-проверка перехода по ссылке, account/password setup, входа и expired/revoked/reused token."
    suggested_fix: "Добавить phone-client/tests/e2e/invite-accept.spec.ts в files_modified и task files, описать перечисленные happy/negative browser сценарии и запуск npm --prefix phone-client run test:e2e -- invite-accept. Focused members tests оставить для быстрой обратной связи. Обновить phase validation strategy."
```

## Предыдущие замечания

| ID | Проверенное исправление | Статус |
|----|------------------------|--------|
| Root DASH-01 | Phase 4 показывает technical counts; unhandled/overdue и точный drill-down впервые добавляются 07-02 | Resolved |
| PRE-01 | jobs/outbox schema, lease/fencing fields и create_intent repository внесены в 02-01; 03-03 расширяет существующую foundation и подключает relay | Resolved по зависимости; размер foundation рассматривается отдельно в FINAL-01 |
| PRE-02 | Все 48 implementation verify обёрнуты automated; manual checkpoints отдельно | Resolved |
| PRE-03 | scripts/smoke-compose.sh и test override строят и запускают изолированный stack, применяют миграции, проверяют readiness/dependencies, очищают только свой Compose project | Resolved; smoke переносится вместе с foundation при FINAL-01 |
| PRE-04 | Добавлен явный /invite/accept route/page и action token→setup/login→accept; expired/revoked/reused состояния описаны | Основной flow resolved; недостающая browser-команда уточняется FINAL-02 |
| PRE-05 | Ссылки указывают на существующие .planning resources; provider-contract обозначен будущим артефактом | Resolved |

## Ограничения этой проверки

Живые запросы Телфина, расшифровка, модельные backend, writes в amoCRM/Sheets, Compose runtime и тесты будущего кода не выполнялись. Доступы, сроки истории, формат каналов, совместимость OpenCode/Codex, качество и стоимость ещё требуют evidence по планам. Пороговые значения качества/нагрузки и бизнес-политика первичности/перезвонов уточняются с владельцем; отсутствие ответа не является подтверждением.

Следующий шаг: revision по FINAL-01/02, затем свежая независимая проверка изменённого пакета и пересчёт общего числа планов. До этого весь пакет не объявлять PASSED; не отмечать выполненными продуктовые требования.
