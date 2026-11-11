---
phase: "05-transcription"
status: implemented_local_verified_asr_quality_pending
verified_at: "2026-11-11"
verifier_role: implementation_review
user_acceptance: pending
live_acceptance: pending
---

# Фаза 5: Аудиопайплайн и расшифровка

**Локальное evidence проверено; фаза целиком не объявляется complete.** Основание — реализация, результаты автоматических/реальных локальных проверок и независимый review API/контрактов. Для собственного shared domain/persistence кода автор этой записи не заявляет независимость; финальный [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен отдельно.

## Evidence реализации

- ASR multipart adapter, проверка magic/size/duration, ffmpeg chunks600 s и versioned transcript реализованы.
- Focused real worker reprocess E2E1/1 PASS; roles E2E1/1 PASS: новая immutable known transcript, исходные segments/manual fields сохранены.

Проверяемые источники: [`phone-worker/src/models.rs`](../../../phone-worker/src/models.rs), [`phone-worker/src/pipeline.rs`](../../../phone-worker/src/pipeline.rs), [`phone-api/src/calls.rs`](../../../phone-api/src/calls.rs), [`phone-client/src/pages/call-detail/CallDetailPage.vue`](../../../phone-client/src/pages/call-detail/CallDetailPage.vue).

| План | Реализованный scope | Статус |
| --- | --- | --- |
| [05-01](05-01-SUMMARY.md) | ASR adapters и long audio | implemented_local_verified_asr_quality_pending |
| [05-02](05-02-SUMMARY.md) | Transcript UI/immutable roles и evaluation gate | implemented_local_verified_asr_quality_pending |

## Незакрытые критерии

- Реальные русские записи, diarization/role accuracy, WER/coverage и long-audio acceptance отсутствуют. Offline scripts/eval-transcripts.py и docs/asr-evaluation.md созданы; размеченный реальный corpus отсутствует, quality acceptance остаётся unknown.

Отсутствие внешней настройки не считается успешной интеграцией. Нет подтверждения, что реальная история полна, ASR распознал речь или модель корректно оценила диалог. Сквозной реестр с командами/метриками/отклонениями: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md), [pilot-report](../../../docs/pilot-report.md).

## Цепочка GSD

[05-RESEARCH.md](05-RESEARCH.md) → соответствующие PLAN → общий независимый [PLAN-CHECK](../../PLAN-CHECK.md) → [REVISION-LOG](../../REVISION-LOG.md) (2 доработки baseline) → summaries/настоящая проверка реализации → [05-UAT.md](05-UAT.md). Execution delta не скрывает несозданные planned artifacts. Финальная пользовательская приёмка и внешние gates открыты; независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен.

Quality harness реализован: [eval-transcripts.py](../../../scripts/eval-transcripts.py), [ASR evaluation](../../../docs/asr-evaluation.md).10 synthetic tests +no-reference CLI smoke PASS; quality acceptance=null.
