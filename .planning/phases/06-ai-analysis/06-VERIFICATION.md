---
phase: "06-ai-analysis"
status: local_contract_runner_verified_model_quality_pending
verified_at: "2026-11-11"
verifier_role: implementation_review
user_acceptance: pending
live_acceptance: pending
---

# Фаза 6: AI-анализ, шкалы и разметка

**Локальное evidence проверено; фаза целиком не объявляется complete.** Основание — реализация, результаты автоматических/реальных локальных проверок и независимый review API/контрактов. Для собственного shared domain/persistence кода автор этой записи не заявляет независимость; финальный [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен отдельно.

## Evidence реализации

- Строгий JSON/evidence/taxonomy validator и deterministic rubric scoring проверены domain tests; unknown/insufficient не становятся0.
- Реальный pinned OpenCode 1.18.10 Docker smoke дважды PASS:401/deny-all/tools=false/MCP0/plugins0/session lifecycle/isolation, model requests0.
- UI версий/доказательств/manual lock и nullable cost проверен unit/E2E.

Проверяемые источники: [`crates/domain/src/scoring.rs`](../../../crates/domain/src/scoring.rs), [`phone-worker/src/models.rs`](../../../phone-worker/src/models.rs), [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`infra/model-runner/Dockerfile`](../../../infra/model-runner/Dockerfile), [`scripts/smoke-model-runner.py`](../../../scripts/smoke-model-runner.py).

| План | Реализованный scope | Статус |
| --- | --- | --- |
| [06-01](06-01-SUMMARY.md) | OpenCode runner/protocol boundary | local_contract_runner_verified_model_quality_pending |
| [06-02](06-02-SUMMARY.md) | Strict analysis/rubric/scoring | local_contract_runner_verified_model_quality_pending |
| [06-03](06-03-SUMMARY.md) | History classification/immutable analyses/UI | local_contract_runner_verified_model_quality_pending |

## Незакрытые критерии

- Реальный model prompt, конкретный Codex/OpenCode provider, held-out оценка руководителя, цена и AI-quality не проверены; offline scripts/eval-analysis.py/docs/analysis-evaluation.md созданы; реальный corpus/quality acceptance отсутствует.

Отсутствие внешней настройки не считается успешной интеграцией. Нет подтверждения, что реальная история полна, ASR распознал речь или модель корректно оценила диалог. Сквозной реестр с командами/метриками/отклонениями: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md), [pilot-report](../../../docs/pilot-report.md).

## Цепочка GSD

[06-RESEARCH.md](06-RESEARCH.md) → соответствующие PLAN → общий независимый [PLAN-CHECK](../../PLAN-CHECK.md) → [REVISION-LOG](../../REVISION-LOG.md) (2 доработки baseline) → summaries/настоящая проверка реализации → [06-UAT.md](06-UAT.md). Execution delta не скрывает несозданные planned artifacts. Финальная пользовательская приёмка и внешние gates открыты; независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен.

Quality harness реализован: [eval-analysis.py](../../../scripts/eval-analysis.py), [analysis evaluation](../../../docs/analysis-evaluation.md).10 synthetic tests +no-reference CLI smoke PASS; quality acceptance=null.
