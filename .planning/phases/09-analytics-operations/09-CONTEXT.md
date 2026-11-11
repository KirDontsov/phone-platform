# Phase 9: Аналитика, аудит и управление обработкой

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Показать качество и потери руководителю, задержки и затраты — владельцу/оператору.

## Decisions

D-01: Aggregate использует тот же фильтр.

D-02: Явные знаменатели/unknown/coverage.

D-03: Audit с авторами и версиями, без raw PII в logs.

D-04: Reprocess не стирает history/manual override.

## Dependencies

Перед execute завершить фазу 8 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Нагрузочный профиль и окончательные KPI бизнеса уточняются по pilot; dashboards не обещают метрики, не измеряемые исходными данными.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 09-01, 09-02 |
| REQ | DASH-02, OPS-01, OPS-02; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 09-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено.
