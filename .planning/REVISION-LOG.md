# Revision log

## Предварительная проверка root-документов — 2026-11-11

Reviewer: отдельный агент `gsd_plan_checker`, роль `gsd-plan-checker`. Планы фаз на этом шаге ещё не проверялись.
Результат до исправления: 0 BLOCKER, 1 WARNING.

```yaml
issues:
  - dimension: dependency_correctness
    severity: warning
    file: ".planning/REQUIREMENTS.md:59"
    finding: "DASH-01 обещает показатель необработанных звонков в фазе 4, но канонический callback engine с учётом успешных контактов, попыток, времени и полноты истории появляется только в фазе 7. До этого семантика показателя не определена."
    suggested_fix: "В фазе 4 явно ограничить базовый дашборд общим количеством и техническими результатами звонков. Показатель необработанных/просроченных обращений и соответствующий drill-down закрепить за фазой 7; не выдавать просто missed за необработанные."
```

Revision 1: DASH-01/phase 4 ограничены техническими метриками. Callback-счётчик и его drill-down закреплены в FLT-03/phase 7. Планировщик получил это изменение до завершения фазовых планов. Повторная проверка включается в итоговый аудит всех фаз.

## Предварительная проверка планов фаз 1–4 — 2026-11-11

11 планов, отдельный checker. Результат: 2 BLOCKER, 3 WARNING. Замечания переданы planner для revision 1. Поздние фазы ещё не входили в этот audit.

```yaml
issues:
  - id: PRE-01
    dimension: dependency_correctness
    severity: blocker
    plan: "03-01, 03-02, 03-03"
    file: ".planning/phases/03-telphin-sync/03-01-PLAN.md:74"
    finding: "03-01 сохраняет атомарный job/outbox intent; 03-02 использует leases и recompute intents. Схема jobs/outbox и repository появляются лишь в следующем 03-03, зависящем от этих планов."
    suggested_fix: "Перенести схему jobs/outbox и минимальный transactional repository в 02-01 или начало 03-01. Relay, consumer, fencing и failure handling можно завершить в 03-03. Указать существующий ранний контракт явно."
  - id: PRE-02
    dimension: nyquist_compliance
    severity: blocker
    plan: "Все проверенные планы"
    file: ".planning/phases/*/*-PLAN.md"
    finding: "RESEARCH содержит Validation Architecture и VALIDATION существует, но проверки task/verify записаны без automated. Это не соответствует статическому требованию GSD 8a."
    suggested_fix: "Обернуть автоматические команды в verify/automated. Live/manual проверки отделить явно. Сохранить честный draft/не запущено; выполнение будущих тестов сейчас не требуется."
  - id: PRE-03
    dimension: verification_quality
    severity: warning
    plan: "02-01"
    file: ".planning/phases/02-platform-auth/02-01-PLAN.md:82"
    finding: "PLAT-01 и must_have обещают запуск всего стека из чистого checkout, но проверки ограничены compose config, cargo check и frontend build. Они не проверят сборку runtime images, миграционный запуск и readiness зависимостей."
    suggested_fix: "Добавить изолированный Compose smoke: build/up --wait, применение миграций, проверки API/readiness и доступности worker dependencies, затем очистка только тестового stack/project. Зафиксировать соответствующий script/config в files_modified."
  - id: PRE-04
    dimension: key_links
    severity: warning
    plan: "02-03"
    file: ".planning/phases/02-platform-auth/02-03-PLAN.md:80"
    finding: "Есть backend invite/accept и экран управления приглашениями владельца, но отсутствует явный browser flow получателя приглашения. Публичной регистрации нет, поэтому новый сотрудник пока не получает описанного пути установить пароль и войти."
    suggested_fix: "Добавить route/page приёма приглашения и вызов accept API с установкой доступа. Проверять от лица нового сотрудника: email link → token validation → account/password setup → login; также expired/revoked/reused token."
  - id: PRE-05
    dimension: source_traceability
    severity: warning
    plan: "Research фаз 1–4; проверить поздние аналогично"
    file: ".planning/phases/02-platform-auth/02-RESEARCH.md:10"
    finding: "Относительные ссылки ../../../research/BONDSCAN.md и ../../../ARCHITECTURE.md выходят в корень проекта, а целевые документы находятся внутри .planning."
    suggested_fix: "Для phase-directory использовать ../../research/*.md и ../../ARCHITECTURE.md. Ссылки на корневой docs/provider-contract.md оставить ../../../docs/provider-contract.md. Проверить существующие локальные ссылки всего пакета."
```

Revision 1 выполнена:

- PRE-01: схема jobs/outbox и минимальный transactional repository перенесены в 02-01; 03-03 расширяет их relay/retry/fencing, а не создаёт впервые.
- PRE-02: автоматические команды задач обёрнуты в `<automated>`; live checkpoints выделены. Draft validation не выдаётся за исполненные тесты.
- PRE-03: в 02-01 добавлен изолированный Compose smoke со сборкой, запуском, миграциями, readiness и очисткой только тестового проекта.
- PRE-04: в 02-03 добавлен маршрут получателя `/invite/accept` и положительный/негативные пользовательские сценарии.
- PRE-05: локальные ссылки исправлены; будущий provider-contract помечен как создаваемый артефакт, а не существующая ссылка.

Дополнительная совместимость GSD: каждый requirement получил отдельную строку Traceability; checkpoint XML содержит поля, требуемые фактической утилитой, и autonomous:false.

Статические проверки после revision: 24/24 plan-structure valid, warnings 0; 24/24 frontmatter valid (проверка planner); 38/38 requirements покрыты, отсутствующих зависимостей и циклов нет; broken Markdown links 0. Всего 48 auto-задач и 2 human checkpoints. Это проверки документов, не прикладной реализации.

Повторная содержательная проверка: передана свежему независимому `gsd_final_verifier`; результат будет в PLAN-CHECK.md. Ни одна фаза реализации пока не закрыта.


## Итоговый аудит пакета после revision 1

Свежий checker `gsd_final_verifier` прочитал все 24 плана. PRE-01…05 подтверждены; осталось 1 BLOCKER и 1 WARNING (2 против предыдущих 5, цикл прогрессирует). Исходный отчёт сохранён в `reviews/01-PLAN-CHECK.md`.

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

Revision 2: разделить foundation на исполнимые планы без скрытия файлов; при необходимости увеличить число планов. Добавить явный browser E2E приглашения. Затем свежий checker проверяет изменения и общий пакет.

Revision 2 выполнена. Вместо механического сохранения числа планов получились 26 планов: фаза 2 содержит Rust scaffold (02-01, 9 файлов), Vue scaffold (02-02, 10), DB/contracts/jobs и Compose smoke (02-03, 14), auth/session (02-04, 14), membership/invite и browser E2E (02-05, 12). Учтены index/Vite/TypeScript config и lockfiles. Foundation остаётся до её использования фазой 3. Обновлены waves/dependencies, summary refs, research/validation и root counts. Invite E2E включает link → setup → login и expired/revoked/reused.

Статические проверки revision 2: 26/26 plan-structure valid без warnings; 26/26 frontmatter valid (planner); 38/38 requirements, unknown IDs 0, отсутствующих зависимостей и циклов 0, broken Markdown links 0. Всего 52 auto-задачи и 2 human checkpoints. Прикладные тесты не запускались.

Независимая повторная проверка revision 2: свежий агент `gsd_revision_verifier`, результат ожидается в PLAN-CHECK.md. У неизменённых фаз сохраняется содержательный аудит предыдущего checker; изменённые фаза 2 и зависимость фазы 3 проверяются повторно.

В ходе этой проверки уточнён test wiring: 02-04 явно создаёт `phone-client/tests/e2e/auth.spec.ts`, запускает Playwright, а не пытается найти тот же файл одновременно через Vitest. В 02-05 заявленный `docs/access-policy.md` включён в files_modified/task files (13 файлов). Проверки структуры двух затронутых планов повторно прошли без замечаний.

## Итог — VERIFICATION PASSED

Свежий `gsd_revision_verifier` подтвердил исправления и обновил PLAN-CHECK.md: **0 BLOCKER, 0 WARNING**. Проверены изменённая фаза 2, dependency фазы 3 и общая статическая целостность 26 планов; содержательный аудит неизменённых фаз сохранён в цепочке reviews. Покрытие 38/38, cycles и broken links отсутствуют. Цикл завершён после двух доработок; продуктовая реализация и live evidence остаются будущими работами.
