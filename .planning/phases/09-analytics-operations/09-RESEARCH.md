# Phase 9: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline ARCHITECTURE.md. Перед execute сверить реальные score/callback/filter semantics и измеренные provider usage. Не усреднять несовместимые rubric versions и не считать unknown нулевым качеством.

## Sources

- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)
- [.planning/research/AI.md](../../../.planning/research/AI.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: Aggregate использует тот же фильтр. D-02: Явные знаменатели/unknown/coverage. D-03: Audit с авторами и версиями, без raw PII в logs. D-04: Reprocess не стирает history/manual override.

Нагрузочный профиль и окончательные KPI бизнеса уточняются по pilot; dashboards не обещают метрики, не измеряемые исходными данными.

## Validation Architecture

- 09-01: Руководитель видит динамику звонков, необработанных обращений и распределение качества. Любая метрика раскрывается в ту же выборку с понятным знаменателем и исключениями. Сценарии и команды — в PLAN.md.
- 09-02: Владелец видит причину задержки импорта/ASR/AI/интеграций и управляет повтором. Изменения правил/разметки/подключений/доступа имеют автора и версию без раскрытия разговоров. Сценарии и команды — в PLAN.md.

Sampling contract — 09-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
