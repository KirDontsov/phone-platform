---
phase: "04-calls-workspace"
plan: "01"
status: local_verified_user_acceptance_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 04-01: Calls API/dashboard и журнал

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования CALL-01, DASH-01 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/pages/calls/CallsPage.vue`](../../../phone-client/src/pages/calls/CallsPage.vue), [`phone-client/src/pages/dashboard/DashboardPage.vue`](../../../phone-client/src/pages/dashboard/DashboardPage.vue), [`phone-client/src/features/call-table/CallTable.vue`](../../../phone-client/src/features/call-table/CallTable.vue).

Отклонение от filenames/объёма исходного [04-01-PLAN.md](04-01-PLAN.md): Запросы/агрегаты выполняются над scoped history в Rust, SQL read-model не создан. Entity-specific call_queries/contracts/calls.ts заменены общими persistence/types/API client.

## Фактическая проверка

- Real API E2E: list/detail/manual annotations/optimistic409/company switch PASS.
- Авторизованный synthetic WAV проигрывается: duration 32, readyState 4, error null; frontend screenshot/mobile QA выполнены.
- Ручной lock и архивные taxonomy IDs сохраняются; чужие ID и media проверяются серверным tenant context.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [04-VERIFICATION.md](04-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Прослушивание и сверка реальной записи АТС; пользовательская оценка удобства. Demo-сигнал не соответствует учебному тексту.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [04-RESEARCH.md](04-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [04-UAT](04-UAT.md) остаётся открытым.
