---
phase: "07-filters-callbacks"
status: local_verified_user_business_rules_pending
verified_at: "2026-11-11"
verifier_role: implementation_review
user_acceptance: pending
live_acceptance: pending
---

# Фаза 7: Конструктор и контроль перезвонов

**Локальное evidence проверено; фаза целиком не объявляется complete.** Основание — реализация, результаты автоматических/реальных локальных проверок и независимый review API/контрактов. Для собственного shared domain/persistence кода автор этой записи не заявляет независимость; финальный [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен отдельно.

## Evidence реализации

- AST validator/evaluator: depth6/nodes64/types/null semantics; frontend builder tests и real API invalidAST/RBAC PASS.
- Callback domain tests: IVR не закрывает, attempts/resolved, incomplete coverage, frozen as_of, weekend/DST.
- HTTP regression: successful callback за пределами выбранной даты не переоткрывает episode; loadcheck точные counts PASS.

Проверяемые источники: [`crates/domain/src/filters.rs`](../../../crates/domain/src/filters.rs), [`crates/domain/src/callbacks.rs`](../../../crates/domain/src/callbacks.rs), [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/features/filter-builder/FilterBuilder.vue`](../../../phone-client/src/features/filter-builder/FilterBuilder.vue).

| План | Реализованный scope | Статус |
| --- | --- | --- |
| [07-01](07-01-SUMMARY.md) | Bounded AST и saved filters | local_verified_user_business_rules_pending |
| [07-02](07-02-SUMMARY.md) | Callback episodes и time rules | local_verified_user_business_rules_pending |

## Незакрытые критерии

- Бизнес-порог/рабочие часы и false-missed/unknown на реальной полной истории нуждаются в UAT. SQL compiler/read models из плана заменены Rust in-memory evaluation.

Отсутствие внешней настройки не считается успешной интеграцией. Нет подтверждения, что реальная история полна, ASR распознал речь или модель корректно оценила диалог. Сквозной реестр с командами/метриками/отклонениями: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md), [pilot-report](../../../docs/pilot-report.md).

## Цепочка GSD

[07-RESEARCH.md](07-RESEARCH.md) → соответствующие PLAN → общий независимый [PLAN-CHECK](../../PLAN-CHECK.md) → [REVISION-LOG](../../REVISION-LOG.md) (2 доработки baseline) → summaries/настоящая проверка реализации → [07-UAT.md](07-UAT.md). Execution delta не скрывает несозданные planned artifacts. Финальная пользовательская приёмка и внешние gates открыты; независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен.
