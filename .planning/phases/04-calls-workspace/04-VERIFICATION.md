---
phase: "04-calls-workspace"
status: local_verified_user_acceptance_pending
verified_at: "2026-11-11"
verifier_role: implementation_review
user_acceptance: pending
live_acceptance: pending
---

# Фаза 4: Журнал, карточка, аудио и разметка

**Локальное evidence проверено; фаза целиком не объявляется complete.** Основание — реализация, результаты автоматических/реальных локальных проверок и независимый review API/контрактов. Для собственного shared domain/persistence кода автор этой записи не заявляет независимость; финальный [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен отдельно.

## Evidence реализации

- Real API E2E: list/detail/manual annotations/optimistic409/company switch PASS.
- Авторизованный synthetic WAV проигрывается: duration 32, readyState 4, error null; frontend screenshot/mobile QA выполнены.
- Ручной lock и архивные taxonomy IDs сохраняются; чужие ID и media проверяются серверным tenant context.

Проверяемые источники: [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-api/src/settings.rs`](../../../phone-api/src/settings.rs), [`phone-client/src/pages/calls/CallsPage.vue`](../../../phone-client/src/pages/calls/CallsPage.vue), [`phone-client/src/pages/call-detail/CallDetailPage.vue`](../../../phone-client/src/pages/call-detail/CallDetailPage.vue).

| План | Реализованный scope | Статус |
| --- | --- | --- |
| [04-01](04-01-SUMMARY.md) | Calls API/dashboard и журнал | local_verified_user_acceptance_pending |
| [04-02](04-02-SUMMARY.md) | Карточка/history/authorized audio | local_verified_user_acceptance_pending |
| [04-03](04-03-SUMMARY.md) | Taxonomy/manual annotations/audit | local_verified_user_acceptance_pending |

## Незакрытые критерии

- Прослушивание и сверка реальной записи АТС; пользовательская оценка удобства. Demo-сигнал не соответствует учебному тексту.

Отсутствие внешней настройки не считается успешной интеграцией. Нет подтверждения, что реальная история полна, ASR распознал речь или модель корректно оценила диалог. Сквозной реестр с командами/метриками/отклонениями: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md), [pilot-report](../../../docs/pilot-report.md).

## Цепочка GSD

[04-RESEARCH.md](04-RESEARCH.md) → соответствующие PLAN → общий независимый [PLAN-CHECK](../../PLAN-CHECK.md) → [REVISION-LOG](../../REVISION-LOG.md) (2 доработки baseline) → summaries/настоящая проверка реализации → [04-UAT.md](04-UAT.md). Execution delta не скрывает несозданные planned artifacts. Финальная пользовательская приёмка и внешние gates открыты; независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен.
