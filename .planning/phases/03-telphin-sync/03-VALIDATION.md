---
phase: "03-telphin-sync"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 3: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. Перед execute актуализировать по созданному scaffold и реальным контрактам, определить latency каждого check и sampling frequency. Не заявлять Nyquist compliance до измерения.

## Test Infrastructure

Rust cargo tests для доменной/контрактной логики; integration tests с отдельной PostgreSQL/RabbitMQ/storage; Vue type-check/build и meaningful UI tests.

## Per-Task Verification

| Plan | Requirement IDs | Checks | Evidence |
|---|---|---|---|
| 03-01 | CONN-01, SYNC-01, SYNC-03 | cargo test -p phone-api telphin_connection; cargo test -p phone-worker --test history_import | SUMMARY с фактическим исходом |
| 03-02 | SYNC-02, SYNC-03 | cargo test -p phone-worker --test reconciliation; cargo test -p phone-api --test telphin_events | SUMMARY с фактическим исходом |
| 03-03 | SYNC-01, SYNC-04 | cargo test -p phone-worker --test job_delivery; cargo test -p phone-worker --test sync_failures | SUMMARY с фактическим исходом |

## Sampling

После каждой доменной/внешней интеграционной задачи запускать её focused check; после плана — перечисленные integration/UI checks; после фазы — verifier по success criteria и регрессии затронутых контрактов. Избегать повторяющих реализацию тестов.

## Manual Gates and Live Evidence

Автоматизируемые проверки выполнить полностью. UAT подтверждает удобство и продуктовые правила; отсутствие реальных внешних credentials фиксируется как непроверенный live gate, не passed.

## Sign-off

- [ ] Focused checks implemented and passed
- [ ] Integration/failure cases passed
- [ ] Required live evidence present
- [ ] Independent verifier and UAT completed
