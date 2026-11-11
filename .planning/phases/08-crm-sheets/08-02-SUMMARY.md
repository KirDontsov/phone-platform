---
phase: "08-crm-sheets"
plan: "02"
status: implemented_local_contract_verified_external_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 08-02: Sheets OAuth/mapping/stable-key upsert

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования SHEET-01, INT-01 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`phone-api/src/operations.rs`](../../../phone-api/src/operations.rs), [`phone-worker/src/providers.rs`](../../../phone-worker/src/providers.rs), [`phone-worker/src/integrations.rs`](../../../phone-worker/src/integrations.rs), [`phone-client/src/pages/settings/SettingsPage.vue`](../../../phone-client/src/pages/settings/SettingsPage.vue), [`docs/providers.md`](../../../docs/providers.md).

Отклонение от filenames/объёма исходного [08-02-PLAN.md](08-02-PLAN.md): Sheets/google OAuth/recovery объединены в providers/integrations/pipeline. RAW writes и stable key реализованы; remote row reorder не атомарен, scope/quota/refresh/external write acceptance pending.

## Фактическая проверка

- OAuth state/exchange/serialized refresh, deal selection, durable deliveries и reconciliation реализованы.
- amoCRM marker, Sheets stable key+RAW mapping и uncertain delivery guards покрыты synthetic provider tests/код-ревью.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [08-VERIFICATION.md](08-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Реальные authorize/scopes/refresh/revocation/quota и доставка в сделку/лист не запускались. Внешние копии автоматически не удаляются; параллельная ручная сортировка Sheets не атомарна.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [08-RESEARCH.md](08-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [08-UAT](08-UAT.md) остаётся открытым.
