# Phase 8: amoCRM и Google Таблицы

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Связать звонки с контактом/сделкой и надёжно обновлять выбранную таблицу.

## Decisions

D-01: Ambiguous deal выбирается вручную.

D-02: Outbox/delivery independent от import/AI.

D-03: Retry без слепого append/повторного note.

D-04: Никакой автоматической смены этапа или лидирования без правила.

## Dependencies

Перед execute завершить фазу 7 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Доступы/приоритет интеграций, структура amoCRM, owner выбранной Sheets. OAuth redirects и scopes требуют deploy config; local fake providers доступны до live.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 08-01, 08-02 |
| REQ | CRM-01, CRM-02, SHEET-01, INT-01; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 08-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено.
