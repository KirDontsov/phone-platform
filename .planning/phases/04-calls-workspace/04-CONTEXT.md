# Phase 4: Журнал, карточка и ручная разметка

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Дать удобный рабочий интерфейс реальных звонков с записями и разметкой до AI.

## Decisions

D-01: Русские предметные состояния.

D-02: Provider result/business status/contact kind/tags/processing отдельны.

D-03: Manual overrides сохраняются.

D-04: Медиа только после tenant/role authorization.

## Dependencies

Перед execute завершить фазу 3 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Дизайн и окончательная taxonomy уточняются на реальных данных; не ждать AI для полезного журнала.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 04-01, 04-02, 04-03 |
| REQ | CALL-01, CALL-02, CALL-03, TAG-01, TAG-02, DASH-01; распределение в frontmatter планов |
| RESEARCH | Каждый constraint в RESEARCH.md должен быть актуализирован и проверен указанными validation сценариями |
| CONTEXT | D-01…D-04 включены в действия планов; явные исключения не добавлять без изменения контекста |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено и не выводится из слова «легирование».
