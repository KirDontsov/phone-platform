# Phase 10: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline ARCHITECTURE.md; перед execute провести targeted review фактических provider retention/deletion APIs, storage lifecycle, backup и deployment. Никакой статус готовности не выводится только из планов.

## Sources

- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)
- [.planning/research/AI.md](../../../.planning/research/AI.md)
- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: Retention удаляет связанные artifacts и производные данные. D-02: Tombstone/fencing запрещают resurrection. D-03: Реальный pilot и held-out quality evidence обязательны. D-04: Release только с известным rollback/restore.

Production host, allowed data policy, фактический объём звонков, качество rubric, RPO/RTO и provider budgets согласовать до release gate.

## Validation Architecture

- 10-01: Настроенный срок хранения удаляет аудио/текст/raw/analysis и учитывает производные внешние данные. Retry и восстановление backup не воскрешают удалённые разговоры. Сценарии и команды — в PLAN.md.
- 10-02: Полный путь от реального звонка до фильтра и выбранной интеграции подтверждён. Качество/нагрузка/ограничения документированы, выпуск и rollback воспроизводимы. Сценарии и команды — в PLAN.md.

Sampling contract — 10-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
