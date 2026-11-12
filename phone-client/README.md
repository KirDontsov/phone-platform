# Сигнал — frontend Phone Platform

Vue 3, TypeScript, Vite, Pinia, Tailwind. Все данные загружаются из `/api`; демонстрационный режим и синтетическая история задаются сервером, не фронтендом.

```bash
npm ci
API_PROXY=http://127.0.0.1:18080 npm run dev -- --port 18082
npm run build
npm test
```

Команда выше запускает dev server на http://localhost:18082, proxy `/api` → http://127.0.0.1:18080 (API из корневого Compose). Этот origin уже разрешён локальным setup. Для другой пары адресов задайте `API_PROXY` и серверный `ALLOWED_ORIGINS`. Dockerfile собирает production SPA с nginx, `/api` проксируется в `api:8080`.

Frontend реализует вход/reset/invite, переключение компаний, dashboard, журнал/пагинацию/конструктор фильтров/сохранённые представления, карточку с аудио/расшифровкой/оценкой/доказательствами/разметкой/историей, callback episodes, настройки сотрудников/критериев/провайдеров/интеграций и операции/audit.

Контракт: `../docs/contracts/platform-v1.md`. Серверная сессия хранится в HttpOnly cookie. CSRF и active organization передаются в API client; generation+AbortController не допускают применения ответов предыдущей компании. Роли ограничивают UI, окончательная проверка прав — на сервере.

```bash
E2E_BASE_URL=http://localhost:18081 E2E_EMAIL=... E2E_PASSWORD=... npm run test:e2e
```

Перед первым запуском установите браузер `npx playwright install chromium`. В окружении с уже установленным Chromium можно задать `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH`.

Playwright использует реальный backend без подмены API. Для authenticated E2E нужны параметры локального тестового пользователя; login/workspace test явно skipped без них. Vitest отдельно проверяет типизацию AST, ограничения сложности и tenant cancellation; e2e-файлы Vitest не подхватывает.

Помощник настройки владельца: `/assistant` или плавающая кнопка; history private user+organization. Телфин credentials вводятся в отдельную форму (включая JSON), Codex APIkey/device flow — в отдельную карточку модели. Обычные сообщения не являются каналом секретов. Браузер получает durable partial messages/events через polling1s; отмена не откатывает применённые настройки. Для реального noauth E2E задайте `E2E_ASSISTANT=1 E2E_ASSISTANT_AUTH=required`; configured automation допускается только с отдельным `E2E_ASSISTANT_FIXTURE=1` и backend model `test-wire-fixture`, без runtime fallback. Подробности: `../docs/assistant-ui.md`.
