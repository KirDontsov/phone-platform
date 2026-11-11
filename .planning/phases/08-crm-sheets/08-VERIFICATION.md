---
phase: "08-crm-sheets"
status: implemented_local_contract_verified_external_pending
verified_at: "2026-11-11"
verifier_role: implementation_review
user_acceptance: pending
live_acceptance: pending
---

# Фаза 8: amoCRM и Google Таблицы

**Локальное evidence проверено; фаза целиком не объявляется complete.** Основание — реализация, результаты автоматических/реальных локальных проверок и независимый review API/контрактов. Для собственного shared domain/persistence кода автор этой записи не заявляет независимость; финальный [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен отдельно.

## Evidence реализации

- OAuth state/exchange/serialized refresh, deal selection, durable deliveries и reconciliation реализованы.
- amoCRM marker, Sheets stable key+RAW mapping и uncertain delivery guards покрыты synthetic provider tests/код-ревью.

Проверяемые источники: [`phone-api/src/operations.rs`](../../../phone-api/src/operations.rs), [`phone-worker/src/providers.rs`](../../../phone-worker/src/providers.rs), [`phone-worker/src/integrations.rs`](../../../phone-worker/src/integrations.rs), [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`phone-client/src/pages/integrations/IntegrationsPage.vue`](../../../phone-client/src/pages/integrations/IntegrationsPage.vue).

| План | Реализованный scope | Статус |
| --- | --- | --- |
| [08-01](08-01-SUMMARY.md) | amoCRM OAuth/link/delivery | implemented_local_contract_verified_external_pending |
| [08-02](08-02-SUMMARY.md) | Sheets OAuth/mapping/stable-key upsert | implemented_local_contract_verified_external_pending |

## Незакрытые критерии

- Реальные authorize/scopes/refresh/revocation/quota и доставка в сделку/лист не запускались. Внешние копии автоматически не удаляются; параллельная ручная сортировка Sheets не атомарна.

Отсутствие внешней настройки не считается успешной интеграцией. Нет подтверждения, что реальная история полна, ASR распознал речь или модель корректно оценила диалог. Сквозной реестр с командами/метриками/отклонениями: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md), [pilot-report](../../../docs/pilot-report.md).

## Цепочка GSD

[08-RESEARCH.md](08-RESEARCH.md) → соответствующие PLAN → общий независимый [PLAN-CHECK](../../PLAN-CHECK.md) → [REVISION-LOG](../../REVISION-LOG.md) (2 доработки baseline) → summaries/настоящая проверка реализации → [08-UAT.md](08-UAT.md). Execution delta не скрывает несозданные planned artifacts. Финальная пользовательская приёмка и внешние gates открыты; независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен.
