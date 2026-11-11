# Phase 6: AI-анализ и авторазметка

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Дать объяснимую оценку разговора и автоматическую смысловую разметку через заменяемый backend моделей.

## Decisions

D-01: Изоляция runner и запрет tools.

D-02: Schema/evidence/IDs валидируются до применения.

D-03: Версии и manual override сохраняются.

D-04: Первичность зависит от historical coverage, а не догадки модели.

## Dependencies

Перед execute завершить фазу 5 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Доступные модели/аккаунты, квоты, бюджет и допустимый error rubric. Codex unavailable честно фиксируется; проверенный альтернативный adapter не скрывает это.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 06-01, 06-02, 06-03 |
| REQ | AI-01, AI-02, AI-03, AI-04, AI-05; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 06-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Пользователь уточнил: речь о тегировании, создание лидов этим не запрашивается.
