# Phase 4: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline ARCHITECTURE.md и Bondscan FSD. Перед execute проверить фактическую форму calls/legs/recording и актуальный range/storage контракт. Публичный storage_url провайдера считается секретом.

## Sources

- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)
- [.planning/research/BONDSCAN.md](../../../.planning/research/BONDSCAN.md)
- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)

## Architectural Responsibility Map

API проверяет сессию, membership и команды; worker выполняет долговременные внешние задачи; PostgreSQL хранит durable state и версии; Vue выводит серверные состояния, а не выдумывает business truth.

## Constraints and Pitfalls

D-01: Русские предметные состояния. D-02: Provider result/business status/contact kind/tags/processing отдельны. D-03: Manual overrides сохраняются. D-04: Медиа только после tenant/role authorization.

Дизайн и окончательная taxonomy уточняются на реальных данных; не ждать AI для полезного журнала.

## Validation Architecture

- 04-01: Пользователь видит все импортированные звонки с понятным исходом и стадией обработки. Счётчики количества, направления и технических исходов раскрываются в тот же список звонков. Сценарии и команды — в PLAN.md.
- 04-02: Карточка показывает маршрут звонка и историю клиента, аудио воспроизводится с перемоткой. Ссылка записи не раскрывает постоянный provider URL и не даёт доступ другой компании. Сценарии и команды — в PLAN.md.
- 04-03: Владелец управляет активной taxonomy, пользователь исправляет статус и теги. Изменение имеет автора/версию и защищено от будущего AI overwrite. Сценарии и команды — в PLAN.md.

Детальный sampling contract — 04-VALIDATION.md. Это стратегия будущей проверки, а не выполненные тесты. Сначала contract/fixture и component checks, затем meaningful integration/failure checks и UAT. Документационные fixtures не заменяют live API/model/pilot evidence.
