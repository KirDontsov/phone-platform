# Phase18 — независимое review авторизации и proxy

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker. Рассмотрены AuthService/config, API model_auth/maintenance, TurnService refresh guards, CONNECT proxy и тесты.

**Статус: PASS после исправления P1 crash consistency при promotion.** Сетевой production device challenge и пользовательский OAuth остаются отдельными live gates.

## P1 — новая credential под старым generation после прерванного promotion

AuthService.complete сохраняет backup и вызывает storage.promote, пока persisted state остаётся authenticated=true с прежним generation. Только затем выставляются in-memory pending.promoted/commitStarted и выполняется API commit. При crash/recreate после замены auth.json до записи нового state status не находит pending process, просто снимает pending_generation и сохраняет старую authenticated=true. Новая credential становится доступной turn под прежней API confirmed_generation. Аналогичный исход возможен при ошибке chmod после rename внутри writePrivate, когда pending.promoted ещё не выставлен.

Исправлено автором и независимо перепроверено: persisted state authenticated=false/commit_uncertain=true/promotion_pending=true записывается ДО storage.promote. Перезапуск и ошибка после rename теперь оставляют turns закрытыми; только подтверждённый новый commit пишет clean authenticated state. При receipt, доказывающем прежнюю active generation, сначала восстанавливается старый auth file, затем старый state; неизвестный commit не откатывается вслепую. Две новые реальные createRunner restart/fault регрессии моделируют исключение после promote и после commit до statewrite; old/new generation turns получают409, новый credential не исполняется под прежней authority. Независимый повтор этих тестов и всей runtime suite PASS.

## Рассмотренные пути — PASS

- Failed login до promotion сохраняет старые credential, runner generation/state и API confirmed_generation. Смена login не отзывает старые runs до успешного commit. Pending TTL/close убирают staging home/process; explicit logout отзывает active+pending authority и удаляет backup.
- API prepare/status/commit capability bound к org/attempt/session/user/deadline. Prepare сохраняет old authority; commit под transaction/locks включает новую generation и отзывает старые runs. Повторный commit idempotent, receipt проверяет действительную committed authority. Maintenance включает новые states и удаляет capability при expiry.
- Потерянный commit response обрабатывается bounded3 retries→status receipt; уже подтверждённую новую credential нельзя вернуть на старую. Unknown outcome закрывает turns и сохраняет private600 backup до явного восстановления/logout. Предварительный persistent marker распространяет это правило на process/storage fault в момент promotion.
- TurnService refresh выполняется под тем же org lock, проверяет generation/authenticated/abort перед promote. MCP/session/tenant guards сохраняются. Revoke/commit сначала меняют DB authority, затем останавливают соответствующие процессы; поздний callback не возвращает доступ.
- Proxy env сконструирован только из fixed local runner+собственного pairing, посторонние inherited proxy/OpenAI/DB secrets не передаются. NO_PROXY задаёт только internal API/runner/localhost; capability callback остаётся direct. CONNECT требует pairing, exact three OpenAI hosts:443, публичный pinned DNS result; body/TLS не расшифровывается и не логируется. DNS/connect/idle/lifetime/socket counts ограничены, shutdown закрывает соединения. Реальный device/model traffic ещё не доказан этими unit tests.

## Тесты

Независимо выполнены обновлённые runtime.test.mjs + proxy-env.test.mjs:22/22 PASS, включая обе новые restart/fault регрессии; gateway/proxy/localprovider suite ранее25/25 PASS. Просмотрен actual isolated PostgreSQL API auth test log:1/1 PASS за133.49s с failed reauth preservation, prepare/commit и logout latecallback. Он запущен автором на настоящем PG; проверяющий не повторял его и не читал секреты/production credentials. Просмотрен RED failed reauthorization на старом коде (authenticated false вместо true). Отдельную RED-мутацию persistent marker проверяющий не запускал; её статус автор фиксирует в общем VERIFICATION.

Чеклист Григория: не проверял, нет чеклиста.
