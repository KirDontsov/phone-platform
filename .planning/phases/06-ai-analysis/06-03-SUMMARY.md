---
phase: "06-ai-analysis"
plan: "03"
status: local_contract_runner_verified_model_quality_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 06-03: History classification/immutable analyses/UI

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования AI-02, AI-03, AI-04, AI-05 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`crates/domain/src/callbacks.rs`](../../../crates/domain/src/callbacks.rs), [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/pages/call-detail/CallDetailPage.vue`](../../../phone-client/src/pages/call-detail/CallDetailPage.vue), [`scripts/eval-analysis.py`](../../../scripts/eval-analysis.py), [`docs/analysis-evaluation.md`](../../../docs/analysis-evaluation.md), [`scripts/tests/test_evaluation.py`](../../../scripts/tests/test_evaluation.py).

Отклонение от filenames/объёма исходного [06-03-PLAN.md](06-03-PLAN.md): contact_history/classify/apply — domain/pipeline; AnalysisView — часть call detail. scripts/eval-analysis.py создан и проверен synthetic tests; held-out human labels отсутствуют. Автоматическая разметка не перетирает manual_lock.

## Фактическая проверка

- Строгий JSON/evidence/taxonomy validator и deterministic rubric scoring проверены domain tests; unknown/insufficient не становятся0.
- Реальный pinned OpenCode 1.18.10 Docker smoke дважды PASS:401/deny-all/tools=false/MCP0/plugins0/session lifecycle/isolation, model requests0.
- UI версий/доказательств/manual lock и nullable cost проверен unit/E2E.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [06-VERIFICATION.md](06-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Реальный model prompt, конкретный Codex/OpenCode provider, held-out оценка руководителя, цена и AI-quality не проверены; offline scripts/eval-analysis.py/docs/analysis-evaluation.md созданы; реальный corpus/quality acceptance отсутствует.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [06-RESEARCH.md](06-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [06-UAT](06-UAT.md) остаётся открытым.

Offline quality harness: scripts/eval-analysis.py;10 synthetic metric/privacy tests PASS. CLI unknown-reference smoke дал acceptance_passed=null; это проверка инструмента, не качества речи/модели.
