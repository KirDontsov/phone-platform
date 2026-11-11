# Phase 5: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline research/AI.md. Перед execute проверить lifecycle выбранного ASR, RU качество, форматы/лимиты, timestamps и diarization. Не закреплять deprecated модель и не приравнивать speaker label к роли менеджера.

## Sources

- [.planning/research/AI.md](../../../.planning/research/AI.md)
- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)
- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)

## Architectural Responsibility Map

API: сессия, membership, commands/queries. Worker: внешние длительные jobs. PostgreSQL: durable state/версии. Vue: серверные состояния и безопасное представление.

## Constraints and Pitfalls

D-01: ASR adapter независим от LLM. D-02: Transcript immutable и versioned. D-03: Неизвестные роли явно unknown. D-04: No audio не исключает call из журнала.

Провайдер/бюджет/retention и фактические mono/stereo каналы. Проверить на разрешённых обезличенных записях, без передачи чужих секретов.

## Validation Architecture

- 05-01: Пригодная запись преобразуется в версионированные сегменты без потери временных смещений. Тишина, повреждение, отсутствие записи и отказ провайдера имеют разные управляемые исходы. Сценарии и команды — в PLAN.md.
- 05-02: Клик по сегменту перематывает запись в правильную позицию. Пользователь видит версию и состояние расшифровки, ручной запуск не стирает прежний результат. Сценарии и команды — в PLAN.md.

Sampling contract — 05-VALIDATION.md. Стратегия будущих проверок, не результаты. Fixtures не заменяют live API/model/pilot evidence.
