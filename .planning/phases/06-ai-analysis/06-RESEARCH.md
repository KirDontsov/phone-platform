# Phase 6: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline research/AI.md, BONDSCAN.md. Обязателен compatibility spike OpenCode V1 из образца против current V2, проверка Codex SDK/CLI headless и доступного аккаунта. Ни один runner ещё не проверен на звонках.

## Sources

- [.planning/research/AI.md](../../../.planning/research/AI.md)
- [.planning/research/BONDSCAN.md](../../../.planning/research/BONDSCAN.md)
- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: Изоляция runner и запрет tools. D-02: Schema/evidence/IDs валидируются до применения. D-03: Версии и manual override сохраняются. D-04: Первичность зависит от historical coverage, а не догадки модели.

Доступные модели/аккаунты, квоты, бюджет и допустимый error rubric. Codex unavailable честно фиксируется; проверенный альтернативный adapter не скрывает это.

## Validation Architecture

- 06-01: Работает проверенный OpenCode/Codex либо другой выбранный adapter с runtime model config. Недоверенный transcript не получает доступ к shell/web/MCP/репозиторию/БД и чужим сессиям. Сценарии и команды — в PLAN.md.
- 06-02: Критерии и веса настраиваются владельцем, итог объясняется доказательствами. Неверная схема, несуществующий evidence или недостаточный transcript не дают подтверждённый score/status. Сценарии и команды — в PLAN.md.
- 06-03: Primary/repeat/unknown объясняется историей до звонка и известным покрытием. Разметка и оценки доступны в карточке; повторный анализ сохраняет ручные решения. Сценарии и команды — в PLAN.md.

Sampling contract — 06-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
