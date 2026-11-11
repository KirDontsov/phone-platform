---
phase: "10-pilot-release"
plan: "01"
status: local_restore_verified_pilot_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 10-01: Retention/deletion ledger/isolated restore

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования OPS-03 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`crates/persistence/src/lib.rs`](../../../crates/persistence/src/lib.rs), [`scripts/backup.py`](../../../scripts/backup.py), [`scripts/tests/test_backup_tools.py`](../../../scripts/tests/test_backup_tools.py), [`docs/backup-restore.md`](../../../docs/backup-restore.md).

Отклонение от filenames/объёма исходного [10-01-PLAN.md](10-01-PLAN.md): backup-platform.sh/restore-platform.sh заменены Python create/verify/ledger. Реальные PG replay/media helper и полный Compose backup→isolated restore PASS. Один retention_days компании, нет отдельных сроков по типам; автоматического удаления CRM/Sheets и встроенного encrypted dump нет.

## Фактическая проверка

- 14 backup safety/restore-flow tests PASS; реальный PG ledger replay и Docker media-plan PASS.
- HTTP load:5000 calls/concurrency 4/32 requests, endpoint p95 517–752 ms; temporary tenants удалены.
- Deletion ledger после backup переигрывается в отдельной restoreDB; deleted call/recursive children/jobs/outbox/media удаляются без изменения sourceDB.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [10-VERIFICATION.md](10-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Полный create→isolated restore smoke Compose PASS; RPO/RTO не согласованы/не измерены. Реальный пилот/UAT/production deployment/quality evaluation не выполнялись. Шифрование backup at rest обеспечивает оператор, встроенного encrypted dump нет; retention один срок компании, не по типам artifact.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [10-RESEARCH.md](10-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [10-UAT](10-UAT.md) остаётся открытым.
