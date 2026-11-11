# Сигнал — frontend Phone Platform

Vue 3, TypeScript, Vite, Pinia, Tailwind. Все данные загружаются из `/api`; демонстрационный режим и синтетическая история задаются сервером, не фронтендом.

```bash
npm ci
npm run dev
npm run build
npm test
```

Dev server: http://localhost:5173, proxy `/api` → http://127.0.0.1:8080. Для другого backend задайте `API_PROXY`. Dockerfile собирает production SPA с nginx, `/api` проксируется в `api:8080`.

Frontend реализует вход/reset/invite, переключение компаний, dashboard, журнал/пагинацию/конструктор фильтров/сохранённые представления, карточку с аудио/расшифровкой/оценкой/доказательствами/разметкой/историей, callback episodes, настройки сотрудников/критериев/провайдеров/интеграций и операции/audit.

Контракт: `../docs/contracts/platform-v1.md`. Серверная сессия хранится в HttpOnly cookie. CSRF и active organization передаются в API client; generation+AbortController не допускают применения ответов предыдущей компании. Роли ограничивают UI, окончательная проверка прав — на сервере.

```bash
E2E_BASE_URL=http://localhost:18081 E2E_EMAIL=... E2E_PASSWORD=... npm run test:e2e
```

Перед первым запуском установите браузер `npx playwright install chromium`. В окружении с уже установленным Chromium можно задать `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH`.

Playwright использует реальный backend без подмены API. Для authenticated E2E нужны параметры локального тестового пользователя; login/workspace test явно skipped без них. Vitest отдельно проверяет типизацию AST, ограничения сложности и tenant cancellation; e2e-файлы Vitest не подхватывает.
