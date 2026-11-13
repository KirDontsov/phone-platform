# Phase20 research

Пользователь: RouterAI выдача ключа завершилась пустой страницей; ключа нет в кабинете. Телфин показывает «Лимит Телфина», в том числе вместо ожидаемых записей.

- Собственный helper завершён routerai_consent_expired; private runtime содержит routerai_saved=false. Нет доказательства получения callback или создания ключа. Причина на стороне consent UI пока неизвестна. Не выдавать истечение ожидания за доказанную причину пустой страницы.
- Callback сейчас возвращает пустое тело при 400/409/410, успех исчезает через 3 секунды. Нужны понятная безопасная страница результата, очистка query и локальная диагностика только enum, без code/state/key/verifier.
- Официальный контракт https://routerai.ru/docs/guides/overview/oauth-pkce проверен 2026-11-13: exact state+iss, S256, одноразовый exchange; повтор того же code может отозвать ключ. Не ослаблять проверки и не повторять exchange. Локальный flow нужен заново; пользовательский consent нельзя имитировать.
- Browser plugin не запускается из-за отсутствующего browser-service.mjs старой версии в runtime; браузер пользователя не проверен. Не читать browser profiles/cookies.
- Телфин: readonly 2026-11-13 08:45 UTC affected tenant: 43370 calls; 33933 recordings; 5187 ready/path, 28746 pending/rate_limited. API ready имеет приоритет над last_error. Нет доказательств удаления или понижения состояния готовых записей.
- claim_job выбирает по next_attempt_at/created_at, не учитывая provider cooldown; десятки тысяч recording retry вытесняют исходный sync и retention. connection_config отклоняет локально уже claimed jobs и расходует попытки без upstream. Нужно пропускать cooldown при claim, дать control jobs приоритет, проверить настоящим изолированным PostgreSQL.
- БД/schema/tenant/auth остаются прежними. Не форсировать retry, не сбрасывать attempts/quota, не вызывать реальные платные модели.

Дополнительная независимая Telphin диагностика: 20-TELPHIN-RESEARCH.md.

## Release finding / revision1

После активации 15-5f633b8 verifier обнаружил уменьшение recordings/ready: существующий retention(default90days) использует возраст звонка started_at, помечает historical calls deleted и ставит purge. Новый приоритет controljobs ускорил эти удаления; исходный sync остался за purge backlog. В09:03UTC root приостановил единственный собственный worker. Это blocker, предыдущая активация не считается успешной финальной проверкой. Требуется explicit owner opt-in автоудаления, live recheck каждого retention purge, безопасное снятие только retention soft-deletion без отмены ручных удалений, инвентаризация уже purged данных и реальных источников восстановления. Нельзя писать «все файлы на месте» после этого finding без новой проверки и разграничения retention.
