# Phase 7: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline ARCHITECTURE.md и TELPHIN.md. Перед execute проверить canonical call result/coverage, identity и все time policies. Missed leg и raw missed counter не равны unhandled episode.

## Sources

- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)
- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: Один typed filter AST для UI/API/агрегатов. D-02: Перезвон-попытка отличается от состоявшегося контакта. D-03: Clock scheduler нужен без новых событий. D-04: Coverage gaps дают uncertainty.

2 часа календарного времени — предложенный default; уточнить business hours/holidays, successful inbound closure и минимальную длительность контакта.

## Validation Architecture

- 07-01: Пользователь комбинирует поля AND/OR/NOT и возвращается к сохранённой выборке. Произвольный filter JSON не превращается в SQL или межорганизационный доступ. Сценарии и команды — в PLAN.md.
- 07-02: Фильтр «пропущен и не обработан N часов» учитывает контакт и попытки, а не только исход звонка. По истечении срока без новых звонков и после late import результат меняется корректно. Сценарии и команды — в PLAN.md.

Sampling contract — 07-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
