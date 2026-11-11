# Phase 3: Подключение и надёжная синхронизация

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Автоматически загружать историю и обновления Телфина без потерь, дублей и ложных пропущенных.

## Decisions

D-01: История — source of truth; события лишь ускоряют.

D-02: Calls/CDR/raw source раздельны.

D-03: Checkpoint после commit.

D-04: Ограниченные retries, outbox и tenant-aware job state.

## Dependencies

Перед execute завершить фазу 2 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Метод webhook auth/retry не документирован достаточно; сначала REST polling, подключение events только после проверки и управления собственными subscriptions.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 03-01, 03-02, 03-03 |
| REQ | CONN-01, SYNC-01, SYNC-02, SYNC-03, SYNC-04; распределение в frontmatter планов |
| RESEARCH | Каждый constraint в RESEARCH.md должен быть актуализирован и проверен указанными validation сценариями |
| CONTEXT | D-01…D-04 включены в действия планов; явные исключения не добавлять без изменения контекста |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено и не выводится из слова «легирование».
