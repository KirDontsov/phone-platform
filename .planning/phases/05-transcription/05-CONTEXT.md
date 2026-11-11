# Phase 5: Аудиопайплайн и расшифровка

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Получать воспроизводимую русскую расшифровку с временными сегментами и честной неопределённостью ролей.

## Decisions

D-01: ASR adapter независим от LLM.

D-02: Transcript immutable и versioned.

D-03: Неизвестные роли явно unknown.

D-04: No audio не исключает call из журнала.

## Dependencies

Перед execute завершить фазу 4 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Open Questions

Провайдер/бюджет/retention и фактические mono/stereo каналы. Проверить на разрешённых обезличенных записях, без передачи чужих секретов.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 05-01, 05-02 |
| REQ | ASR-01, ASR-02; распределение в frontmatter планов |
| RESEARCH | Ограничения и проверки указаны в действиях планов и 05-VALIDATION.md |
| CONTEXT | D-01…D-04 включены в действия планов; исключения не добавлять молча |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Пользователь уточнил: речь о тегировании, создание лидов этим не запрашивается.
