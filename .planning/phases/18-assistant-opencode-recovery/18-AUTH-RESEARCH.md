# Phase18 — независимое исследование авторизации Codex

Дата: 2026-11-12. Проверяющий: assistant_auth_diagnosis. Production изучен только для Phone; credentials, auth.json, пользовательские тексты и телефоны не читались/не выводились. Worker/импорт не перезапускались.

## Подтверждённая причина

Вход Codex по ChatGPT device code на удалённом сервере завершается **upstream HTTP403 Forbidden**. Ошибка скрывается адаптером `rpc.mjs` как общий `codex_request_failed` и не связана с требованием OpenRouter API-ключа в другой форме.

Сравнение публичных endpoint без авторизации:

| Проба | Собственный remote assistant container | Этот локальный компьютер |
| --- | --- | --- |
| GET `https://auth.openai.com/.well-known/openid-configuration` | 403 | 200 |
| GET `https://chatgpt.com/backend-api/codex/models` без auth | 403 | 401, ожидаемое отсутствие авторизации |

Ответы проверялись в памяти, выводились только HTTP-код и безопасные логические признаки. HTML/JSON body не публиковались. Признаки Cloudflare HTML, явного unsupported region и device-code-disabled не обнаружены; конкретный механизм отказа/геолокация не установлены.

С разрешения root выполнен настоящий изолированный probe закреплённого Codex0.167.2 в текущем remote image: новый собственный `/tmp/phone-auth-diagnostic-*`, пустой home, штатный `configText()`, `initialize`, затем `account/login/start({type:chatgptDeviceCode})`. Initialize проходит. Login возвращает JSON-RPC **-32603**, безопасная классификация: HTTP403=true, forbidden=true, network=true, HTTP429=false, device-disabled=false. Успешный device challenge не получен. CLI немедленно остановлен, только собственная временная папка удалена. API/DB/state существующей авторизации не менялись; model request не выполнялся.

## Состояние затронутой компании

- В её Phone DB `assistant_auth_control` впервые создан 19:02:37UTC, после выпуска17; текущее authenticated=false/confirmed_generation=null. Последняя попытка 19:05:06UTC failed, предыдущие3 затем revoked; все способы — chatgptDeviceCode.
- Собственный runner `state.json`: authenticated=false, auth_mode=null, error_code=codex_request_failed. Для `auth.json` проверены только stat/существование: **файл отсутствует**, содержимое не читалось.
- Для этой компании `assistant_run` не найден. Поэтому нельзя утверждать, что обновление потеряло её ранее сохранённую авторизацию или что сломался refresh во время диалога. Smoke старого demo owner проверяет **другую компанию**, к этой компании его authenticated=true не относится.
- Persisted Phone `assistant_auth` volume в Compose сохраняется при recreate. Заимствовать авторизацию старого owner, другого tenant, host Codex или Bondscan нельзя.

## Код и контракт

- Настоящая JSON Schema текущего CLI, сгенерированная offline во временный собственный каталог, подтверждает используемые `chatgptDeviceCode`, `loginId`, `userCode`, `verificationUrl`. Причина не в угаданной форме RPC.
- `TurnService` **уже сохраняет обновлённый auth после успешного turn** (`credentials_refresh` → storage.promote под generation guard). Гипотеза «никогда не сохраняет refresh» неверна. После failed turn обновлённые credentials действительно могут остаться только во временном home, но текущая компания до turn не дошла; это отдельный возможный риск, не подтверждённая причина этого инцидента.
- `AuthService.start` удаляет существующий auth до начала новой попытки. Без доказательств прежнего файла нельзя приписывать этому текущую потерю, однако failed reauthorization способна удалить сохранённый credential. Следует отдельно продумать сохранение подтверждённой auth до успешной замены без обхода tenant/generation/revoke guards.
- `CodexRpc.message` отбрасывает все подробности upstream error, сохраняя один generic code. Пользователь получает предложение повторить вход, хотя сервер стабильно отказывается по сети. Нужна безопасная классификация HTTP403/429/network и понятное сообщение без raw body/token.

## Рекомендация к плану

1. Провести Codex auth **и дальнейший model traffic** через рабочий локальный компьютер, как пользователь просил для Docker analyzer: отдельный локальный runner через существующий Phone tunnel либо явно ограниченный egress proxy. Один только перенос device-code запроса недостаточен: удалённый model endpoint также отвечает403 даже на anonymous request.
2. API key не является исправлением этой сетевой причины. Не заставлять повторно входить до исправления пути к OpenAI, не использовать чужие auth/секреты.
3. Проверить настоящий OAuth endpoint через конечный транспорт; затем дать пользователю завершить одноразовый интерактивный device login. Не объявлять authenticated/успешный model turn по одной health/OIDC-пробе.
4. Добавить безопасную диагностику upstream failure и реальные transport regressions. Сохранять scoped generation/session/capability, текущую изоляцию MCP и отдельность конфигурационного помощника от анализатора разговоров.

Изменения приложения/production в рамках этого исследования не делались. Исследование OpenCode/ASR authentication выполняет root отдельно.
