# Phase 3: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline research/TELPHIN.md и фактический docs/provider-contract.md фазы 1. Перед execute перенести только подтверждённые contracts/limits, повторно проверить current docs и subscriptions/auth.

## Sources

- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)
- `docs/provider-contract.md` — будущий артефакт фазы 1, прочитать перед execute фазы 3.

## Architectural Responsibility Map

API проверяет сессию, membership и команды; worker выполняет долговременные внешние задачи; PostgreSQL хранит durable state и версии; Vue выводит серверные состояния, а не выдумывает business truth.

## Constraints and Pitfalls

D-01: История — source of truth; события лишь ускоряют. D-02: Calls/CDR/raw source раздельны. D-03: Checkpoint после commit. D-04: Ограниченные retries, outbox и tenant-aware job state.

Метод webhook auth/retry не документирован достаточно; сначала REST polling, подключение events только после проверки и управления собственными subscriptions.

## Validation Architecture

- 03-01: Владелец проверяет права АТС, выбирает период и видит состояние импорта. Повторный импорт хранит один логический звонок с корректными плечами и coverage. Сценарии и команды — в PLAN.md.
- 03-02: Новый звонок и поздняя запись появляются даже при недоставленном webhook. Дубликаты и перестановка событий не портят результат звонка. Сценарии и команды — в PLAN.md.
- 03-03: Crash между БД и брокером не теряет задачу, redelivery не создаёт дубликат. Истёкший токен и лимит API видны как управляемое состояние подключения. Сценарии и команды — в PLAN.md.

Детальный sampling contract — 03-VALIDATION.md. Это стратегия будущей проверки, а не выполненные тесты. Сначала contract/fixture и component checks, затем meaningful integration/failure checks и UAT. Документационные fixtures не заменяют live API/model/pilot evidence.
