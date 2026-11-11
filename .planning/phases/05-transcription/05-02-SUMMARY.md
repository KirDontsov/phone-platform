---
phase: "05-transcription"
plan: "02"
status: implemented_local_verified_asr_quality_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 05-02: Transcript UI/immutable roles и evaluation gate

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования ASR-01, ASR-02 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/pages/call-detail/CallDetailPage.vue`](../../../phone-client/src/pages/call-detail/CallDetailPage.vue), [`phone-client/tests/transcript-roles.test.ts`](../../../phone-client/tests/transcript-roles.test.ts), [`phone-client/tests/e2e/transcript-roles.spec.ts`](../../../phone-client/tests/e2e/transcript-roles.spec.ts), [`scripts/eval-transcripts.py`](../../../scripts/eval-transcripts.py), [`docs/asr-evaluation.md`](../../../docs/asr-evaluation.md), [`scripts/tests/test_evaluation.py`](../../../scripts/tests/test_evaluation.py).

Отклонение от filenames/объёма исходного [05-02-PLAN.md](05-02-PLAN.md): Transcript API встроен в call detail; роли подтверждаются новой версией с analyze job. docs/asr-evaluation.md и scripts/eval-transcripts.py созданы;10 synthetic harness tests PASS, real annotated corpus/quality pending.

## Фактическая проверка

- ASR multipart adapter, проверка magic/size/duration, ffmpeg chunks600 s и versioned transcript реализованы.
- Focused real worker reprocess E2E1/1 PASS; roles E2E1/1 PASS: новая immutable known transcript, исходные segments/manual fields сохранены.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [05-VERIFICATION.md](05-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Реальные русские записи, diarization/role accuracy, WER/coverage и long-audio acceptance отсутствуют. Offline scripts/eval-transcripts.py и docs/asr-evaluation.md созданы; размеченный реальный corpus отсутствует, quality acceptance остаётся unknown.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [05-RESEARCH.md](05-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [05-UAT](05-UAT.md) остаётся открытым.

Offline quality harness: scripts/eval-transcripts.py;10 synthetic metric/privacy tests PASS. CLI unknown-reference smoke дал acceptance_passed=null; это проверка инструмента, не качества речи/модели.
