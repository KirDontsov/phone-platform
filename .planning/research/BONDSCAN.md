# Bondscan: проверенные образцы

Исследовано локально 2026-11-11. Образец: `/Users/kirilldoncov/Documents/my/obligation-platform`. Доказательство связи с Bondscan: `obligation-analyzer/src/config.rs` ссылается на production API `api.bondscan.ru`.

## Что действительно используется

| Область | Проверенный источник | Вывод |
|---------|---------------------|-------|
| Backend | `obligation-api/Cargo.toml`, `ai/docs/architecture.md` | Rust, Actix Web, Diesel/PostgreSQL, Tokio, reqwest, lapin |
| Frontend | `obligation-client/package.json`, `README.md` | Vue 3, TypeScript, Vite, Pinia, vue-router, Tailwind v4 |
| FSD | `obligation-client/ai/docs/fsd-architecture-rules.md` | shared/entities/features/pages, централизованный API, stores для состояния |
| Auth | root `.planning/codebase/ARCHITECTURE.md`, client README | Cookie-auth и route guards; реализацию отзыва/CSRF нового продукта проектировать отдельно |
| Worker | `obligation-analyzer/Cargo.toml`, `src/consumer.rs`, `src/analyzer.rs` | Отдельный Rust consumer, RabbitMQ, manual ACK/DLQ/backoff |
| Model dispatch | `src/analyzer.rs`, `src/tournament.rs` | Общий `run_opencode`, CLI fallback; локальные fast paths уже имеют собственные особенности |
| Evaluation | `src/analyzer.rs`, `src/judge.rs` | Дополнительная проверка модельного результата, retries и deterministic guards |
| Packaging | `obligation-analyzer/Dockerfile` | Multi-stage Rust image, OpenCode binary, non-root, runtime credentials |
| Permissions | `obligation-analyzer/opencode.json` | Реальный конфиг запрещает bash/edit/task/webfetch/websearch; read по умолчанию ask |

## Переносим

Разделение SPA/API/worker; привычный стек; job progress; provider/model configuration; runtime secrets; durable queues и отдельную обработку ошибок; FSD и унифицированные loading/error состояния.

## Адаптируем

- У Bondscan отдельные репозитории; здесь предлагается один репозиторий с тремя приложениями и shared crates. Решение не меняет deployment boundaries.
- Июньская root-карта описывает вызовы AI внутри API; актуальный analyzer уже вынесен. Приоритет фактическому коду.
- Комментарий Dockerfile допускает webfetch для старой задачи, но актуальный opencode.json его запрещает. Комментарии не являются контрактом безопасности.
- В Dockerfile OpenCode ставится через live installer; для нового сервиса закрепляем проверенный release/digest.
- В коде Bondscan есть recovery повреждённого JSON; для статусов/оценок разговоров принимаем только прошедший строгую валидацию результат, иначе bounded retry/needs_review.
- Финансовые prompts, таблицы, auth accounts, локальные токены, prod инфраструктура и домены не копируются.

## Текст интерфейса

Русский, краткий, объясняющий результат: «Запись ещё готовится», «Разговор не состоялся», «Недостаточно истории для определения первичного обращения», «Нужна проверка», «Попытка перезвона без ответа». Никаких opencode/job/JSON в обычном пользовательском сценарии; технические детали — на экране администратора.
