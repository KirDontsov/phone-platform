---
phase: "01-telphin-api-spike"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 1: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. Перед execute актуализировать по созданному scaffold и реальным контрактам, определить latency каждого check и sampling frequency. Не заявлять Nyquist compliance до измерения.

## Test Infrastructure

Python stdlib unittest и документированные обезличенные API fixtures.

## Per-Task Verification

| Plan | Requirement IDs | Checks | Evidence |
|---|---|---|---|
| 01-01 | TEL-01, TEL-02 | python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py'; python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py' | SUMMARY с фактическим исходом |
| 01-02 | TEL-02, TEL-03 | python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py'; python3 scripts/telphin-spike.py probe | SUMMARY с фактическим исходом |

## Sampling

После каждой доменной/внешней интеграционной задачи запускать её focused check; после плана — перечисленные integration/UI checks; после фазы — verifier по success criteria и регрессии затронутых контрактов. Избегать повторяющих реализацию тестов.

## Manual Gates and Live Evidence

- Проверить подготовленный обезличенный completeness-report против выбранного периода в кабинете Телфина. Подтвердить охват АТС и объяснение расхождений. До реального доступа и сверки фазу 1 не закрывать.

## Sign-off

- [ ] Focused checks implemented and passed
- [ ] Integration/failure cases passed
- [ ] Required live evidence present
- [ ] Independent verifier and UAT completed
