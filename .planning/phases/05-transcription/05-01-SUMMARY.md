---
phase: "05-transcription"
plan: "01"
status: implemented_local_verified_asr_quality_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 05-01: ASR adapters и long audio

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования ASR-01, ASR-02 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`phone-worker/src/models.rs`](../../../phone-worker/src/models.rs), [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`phone-worker/Dockerfile`](../../../phone-worker/Dockerfile), [`docs/providers.md`](../../../docs/providers.md).

Отклонение от filenames/объёма исходного [05-01-PLAN.md](05-01-PLAN.md): asr/provider/pipeline/audio_prepare/segments объединены в models.rs/pipeline.rs; ffmpeg в worker Dockerfile. Роли unknown по умолчанию. Real ASR quality, русский long/stereo corpus и его acceptance пока отсутствуют.

## Фактическая проверка

- ASR multipart adapter, проверка magic/size/duration, ffmpeg chunks600 s и versioned transcript реализованы.
- Focused real worker reprocess E2E1/1 PASS; roles E2E1/1 PASS: новая immutable known transcript, исходные segments/manual fields сохранены.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [05-VERIFICATION.md](05-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Реальные русские записи, diarization/role accuracy, WER/coverage и long-audio acceptance отсутствуют. Offline scripts/eval-transcripts.py и docs/asr-evaluation.md созданы; размеченный реальный corpus отсутствует, quality acceptance остаётся unknown.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [05-RESEARCH.md](05-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [05-UAT](05-UAT.md) остаётся открытым.

Offline quality harness: scripts/eval-transcripts.py;10 synthetic metric/privacy tests PASS. CLI unknown-reference smoke дал acceptance_passed=null; это проверка инструмента, не качества речи/модели.
