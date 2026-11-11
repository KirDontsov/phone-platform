# Phase 8: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline research/TELPHIN.md section 10. Перед execute изучить current official OAuth/scopes/endpoints и идемпотентность amoCRM/Google. Подтверждённые quota values не заменяют response-aware throttling.

## Sources

- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)
- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: Ambiguous deal выбирается вручную. D-02: Outbox/delivery independent от import/AI. D-03: Retry без слепого append/повторного note. D-04: Никакой автоматической смены этапа или лидирования без правила.

Доступы/приоритет интеграций, структура amoCRM, owner выбранной Sheets. OAuth redirects и scopes требуют deploy config; local fake providers доступны до live.

## Validation Architecture

- 08-01: Пользователь видит выбранную связанную сделку; неоднозначный результат не назначается случайно. Повтор после timeout не создаёт второе примечание о том же звонке. Сценарии и команды — в PLAN.md.
- 08-02: Выбранные поля звонка обновляют строку по стабильному ключу, перестановка строк не создаёт дублей. Отключённый доступ и quota видны, сбой таблицы не мешает звонкам. Сценарии и команды — в PLAN.md.

Sampling contract — 08-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
