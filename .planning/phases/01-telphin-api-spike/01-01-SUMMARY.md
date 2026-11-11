---
phase: "01-telphin-api-spike"
plan: "01"
status: completed_offline
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 01-01: Read-only CLI и durable export

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования TEL-01, TEL-02 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`scripts/telphin-spike.py`](../../../scripts/telphin-spike.py), [`scripts/tests/test_telphin_spike.py`](../../../scripts/tests/test_telphin_spike.py), [`docs/telphin-spike.md`](../../../docs/telphin-spike.md).

Отклонение от filenames/объёма исходного [01-01-PLAN.md](01-01-PLAN.md): SQLite checkpoint используется только в CLI; production persistence — отдельный PostgreSQL. Исторический CLI commit 874ca1d сохранён; платформа/ops/accessaudit зафиксированы в e789168/7af83b7/3563c4f.

## Фактическая проверка

- 56 offline CLI tests PASS: OAuth/scope, gzip/limits, pagination/checkpoints, binding и reconciliation.
- Запросы аккаунта Телфина не выполнялись; M0 открыт.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [01-VERIFICATION.md](01-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Реальный probe/history/recording, границы времени, depth и независимая сверка АТС.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [01-RESEARCH.md](01-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [01-UAT](01-UAT.md) остаётся открытым.
