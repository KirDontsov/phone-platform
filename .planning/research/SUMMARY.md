# Research summary

2026-11-11. Исследование документов и локального кода; live API и модели не запускались.

1. [Bondscan](BONDSCAN.md): подтверждён стек Rust/Actix/Diesel/PostgreSQL + Vue/Pinia/Tailwind и отдельный RabbitMQ/OpenCode analyzer. Можно использовать архитектурные приёмы, не переносить домен и секреты.
2. [Телфин](TELPHIN.md): официальный API предоставляет историю, записи и события. Первым делом подтвердить OAuth/permissions/пагинацию/границы и результат calls против cdr на своём аккаунте.
3. [AI](AI.md): Codex автоматизируется через SDK; оценка разговоров требует отдельной ASR и проверки качества. OpenCode V1/V2 — явная граница совместимости.
4. Ключевые риски: неполная история; missed shoulders при answered call; поздние записи; неопределённость speaker roles; потеря ручной разметки; дубли CRM/Sheets после retry; просрочка callback без новых событий.
5. Решение: metadata-first импорт; immutable source/transcript/analysis versions; отдельные состояния; outbox/idempotency; deterministic history/callback rules; AI suggestions со строгой схемой и evidence.

Следующий evidence milestone — фаза 1. Без него нельзя заявлять, что получены все звонки, или переносить угаданные provider limits в production.
