# Phase20 independent OAuth / frontend implementation review

2026-11-13. Объём: scripts/connect-routerai.py, scripts/tests/test_connect_routerai.py, phone-client/src/shared/lib/recording-state.ts и соответствующие UI tests. Worker/claim SQL проверяются отдельно. Реальный RouterAI account consent, причина пустой страницы провайдера и production release этой проверкой не установлены.

## Первичная проверка: BLOCKED → повторная проверка PASS

1. **P1 — истёкший flow продолжает показывать ожидание.** `main()` по истечении deadline присваивает `flow.result={state:error,error:routerai_consent_expired}`, но не выставляет `flow.done`. `/connected` проверяет `not flow.done.is_set()` раньше result, поэтому в течение новых 60 секунд grace показывает «Ожидается подтверждение…» и refresh. Это противоречит задаче понятного terminal результата. Нужно сигнализировать terminal timeout через done и добавить lifecycle regression, не только прямое посещение `/problem/410`.
2. **P2 — существующая UI проверка не обновлена.** recording-table.test.ts ожидает «Лимит Телфина», новый UI выводит «Ожидает Телфин». Фактический scoped run: 1 failed,4 passed. Обновить семантическое ожидание и проверить пояснение автоматической паузы.

## Что проверено

- Python actual localhost HTTP suite: **13/13 PASS**. Exact Host/state/issuer/duplicate/expiry/replay guards сохранены, нет ослабления проверки и повторного exchange при timeout. Неверный callback не расходует валидную попытку.
- Callback 400/404/409/410 теперь отдаёт непустой HTML; meta Refresh ведёт только на фиксированный `/problem/<status>`, без query. Чужой Host не перенаправляется. Cache-Control no-store / Referrer-Policy no-referrer / CSP сохранены. Error result enum выводится только через allowlist, unknown string не отражается.
- Existing one-time exchange, no upstream redirects, private600 runtime merge/tenant binding/rollback tests проходят. Access logs подавлены; новые страницы не включают code/state/verifier/key.
- Consent default600s и completion grace60s ограничены. Успех определяется завершением commit/restart, не callback arrival.
- Новый badge меняет только label/description, не серверный enum/фильтры. Ready playback presentation не меняется. recording-filters tests:3/3 PASS.
- Scoped команды: `python3 -m unittest discover -s scripts/tests -p test_connect_routerai.py -v`; `cd phone-client && npm test -- tests/recording-table.test.ts tests/recording-filters.test.ts`.

RED новых тестов независимый reviewer не проверял. Browser navigation meta Refresh не проверена живым браузером; HTTP fixture проверяет разметку/target и отдельно clean page. Для выпуска необходим повтор после устранения замечаний.

## Повторная проверка после исправлений

Оба замечания закрыты: main timeout теперь выставляет `flow.done.set()`, новый actual HTTP lifecycle test запускает main с истёкшим deadline и проверяет `/connected` во время completion grace — terminal routerai_consent_expired, без ожидания, main завершён code1. UI ожидание обновлено на «Ожидает Телфин».

Независимый повтор: Python **14/14 PASS**, recording-table/recording-filters **5/5 PASS**. Security guards предыдущей проверки проходят без изменений. ROOT scope review — **PASS**. Worker/persistence review и production verification остаются отдельными gate. Reviewer RED mutation lifecycle теста не выполнял; оператор проверяет отдельно.
