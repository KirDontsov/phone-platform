---
phase: "04-calls-workspace"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 4: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. Перед execute актуализировать по созданному scaffold и реальным контрактам, определить latency каждого check и sampling frequency. Не заявлять Nyquist compliance до измерения.

## Test Infrastructure

Rust cargo tests для доменной/контрактной логики; integration tests с отдельной PostgreSQL/RabbitMQ/storage; Vue type-check/build и meaningful UI tests.

## Per-Task Verification

| Plan | Requirement IDs | Checks | Evidence |
|---|---|---|---|
| 04-01 | CALL-01, DASH-01 | cargo test -p phone-api --test calls_list; npm --prefix phone-client run build | SUMMARY с фактическим исходом |
| 04-02 | CALL-02, CALL-03 | cargo test -p phone-api --test recording_access; npm --prefix phone-client run type-check | SUMMARY с фактическим исходом |
| 04-03 | TAG-01, TAG-02 | cargo test -p phone-api --test annotations; npm --prefix phone-client run build | SUMMARY с фактическим исходом |

## Sampling

После каждой доменной/внешней интеграционной задачи запускать её focused check; после плана — перечисленные integration/UI checks; после фазы — verifier по success criteria и регрессии затронутых контрактов. Избегать повторяющих реализацию тестов.

## Manual Gates and Live Evidence

Автоматизируемые проверки выполнить полностью. UAT подтверждает удобство и продуктовые правила; отсутствие реальных внешних credentials фиксируется как непроверенный live gate, не passed.

## Sign-off

- [ ] Focused checks implemented and passed
- [ ] Integration/failure cases passed
- [ ] Required live evidence present
- [ ] Independent verifier and UAT completed
