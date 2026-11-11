# Phase 7: Фильтры и контроль перезвонов

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Сохранить произвольные выборки и находить реально необработанные обращения через заданное время.

## Decisions

D-01: Один typed filter AST для UI/API/агрегатов.

D-02: Перезвон-попытка отличается от состоявшегося контакта.

D-03: Clock scheduler нужен без новых событий.

D-04: Coverage gaps дают uncertainty.

## Dependencies

Перед execute завершить фазу 6 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

2 часа календарного времени — предложенный default; уточнить business hours/holidays, successful inbound closure и минимальную длительность контакта.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 07-01, 07-02 |
| REQ | FLT-01, FLT-02, FLT-03, FLT-04; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 07-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено.
