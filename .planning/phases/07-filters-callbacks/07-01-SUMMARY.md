---
phase: "07-filters-callbacks"
plan: "01"
status: local_verified_user_business_rules_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 07-01: Bounded AST и saved filters

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования FLT-01, FLT-02 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`crates/domain/src/filters.rs`](../../../crates/domain/src/filters.rs), [`phone-api/src/settings.rs`](../../../phone-api/src/settings.rs), [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/shared/lib/filter.ts`](../../../phone-client/src/shared/lib/filter.ts), [`phone-client/src/features/filter-builder/FilterBuilder.vue`](../../../phone-client/src/features/filter-builder/FilterBuilder.vue).

Отклонение от filenames/объёма исходного [07-01-PLAN.md](07-01-PLAN.md): SQL compiler/read-model из плана не реализован: один Rust validator/evaluator применяется к scoped JSON calls и dashboard. SQL injection через AST исключён отсутствием SQL генерации; перенос в SQL требует новой проверки semantics/performance.

## Фактическая проверка

- AST validator/evaluator: depth6/nodes64/types/null semantics; frontend builder tests и real API invalidAST/RBAC PASS.
- Callback domain tests: IVR не закрывает, attempts/resolved, incomplete coverage, frozen as_of, weekend/DST.
- HTTP regression: successful callback за пределами выбранной даты не переоткрывает episode; loadcheck точные counts PASS.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [07-VERIFICATION.md](07-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Бизнес-порог/рабочие часы и false-missed/unknown на реальной полной истории нуждаются в UAT. SQL compiler/read models из плана заменены Rust in-memory evaluation.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [07-RESEARCH.md](07-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [07-UAT](07-UAT.md) остаётся открытым.
