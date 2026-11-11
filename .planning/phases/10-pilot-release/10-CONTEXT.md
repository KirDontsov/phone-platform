# Phase 10: Хранение, пилот и выпуск

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Проверить полный путь, качество, нагрузку и восстановление, затем подготовить воспроизводимый выпуск.

## Decisions

D-01: Retention удаляет связанные artifacts и производные данные.

D-02: Tombstone/fencing запрещают resurrection.

D-03: Реальный pilot и held-out quality evidence обязательны.

D-04: Release только с известным rollback/restore.

## Dependencies

Перед execute завершить фазу 9 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Production host, allowed data policy, фактический объём звонков, качество rubric, RPO/RTO и provider budgets согласовать до release gate.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 10-01, 10-02 |
| REQ | OPS-03, REL-01; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 10-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено.
