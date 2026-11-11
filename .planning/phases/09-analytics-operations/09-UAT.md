---
phase: "09-analytics-operations"
status: pending_user_acceptance
updated_at: "2026-11-11"
---

# UAT фазы 9: Аналитика, аудит и операции

**Пользователь ещё не проводил финальную приёмку.** Автоматические результаты находятся в [09-VERIFICATION.md](09-VERIFICATION.md); они не заменяют подтверждение пользователя и live evidence.

- [ ] Сверить dashboard counts/period/filter с журналом и проверить unknown average/cost.
- [ ] Проверить audit actor/version, retry/worker heartbeat/budget и согласовать оперативные alerts/квоты.

Общий сценарий: [docs/uat.md](../../../docs/uat.md). Для реальных аккаунтов ключи вводятся только локально; секреты/аудио/номера не добавляются в Git или обычные логи. Demo audio/text не подтверждают качество ASR/AI. Результат фиксируется как дата, проверенные пункты и конкретные расхождения; отсутствие ответа не является acceptance.
