# Независимая проверка реализации — фаза14

Дата: 2026-11-12. Проверяющий: phase14_checker, отдельный агент. Production source не редактировал. Проверены план, финальные исходники, контракты/инструкции, отчёты исполнителей и фактические GREEN/RED/SSH/Docker/browser artifacts. PLAN-CHECK не считается evidence готовности реализации.

**Локальная реализация: PASS. Реальный удалённый сервер, внешнее ASR-качество и финальный пользовательский UAT: PENDING.** Локальный connector запущен на этом компьютере; рабочий SSH-туннель к пользовательскому серверу не объявляется подключённым без target/pairing.

## Что подтверждено

| Требование | Фактический результат | Граница |
|---|---|---|
| Удалённый сайт → туннель → локальный OpenCode | Remote worker сохраняет DB/queue/media; Docker alias model-runner у private SSH relay ведёт через2 SSH hops в local connector | Транспорт доказан на настоящем isolated Docker/SSH, не на пользовательском remote host |
| One-command start-analyzer | init/up/local/down/status/doctor, отдельный phone-analyzer project, собственные keys/pairing/pins, bounded reconnect; local startup healthy | Повседневный up требует одноразового remote pairing; пока запущен только local connector |
| Обе точные ASR модели | moondream/parakeet-ultra и nvidia/nemotron-3.5-transcribe в настоящем v1.18.10 opencode.json и UI | Native OpenCode не имеет STT API; ASR выполняется явным расширением Docker connector, не вымышленным chat/session inference |
| Правильный STT протокол | Worker→/asr/transcriptions→fixed OpenRouter /api/v1/audio/transcriptions, JSON raw base64, verbose_json; без model substitution или скрытого повторного платного запроса | Реальный inference выбранного аккаунта и поддержка timestamps/diarization endpoint не проверены |
| Авторизация и изоляция | Runtime runner password отдельно от encrypted tenant OpenRouter key; trusted operator URL, separate request header, no native forwarding/cache/env fallback; provider/base change требует новый key | Рабочий ключ OpenRouter вводит владелец; авторизация Codex не подменяет его |
| Ограничения и история | Auth/concurrency до чтения body, request/audio/response/time limits, abort, safe errors, numeric speaker identities без угадывания ролей, usage/null; предыдущие результаты/manual overrides сохранены | Quality/стоимость на customer corpus не доказаны fixtures |

## Исправления независимого review

1. Короткий ASR deadline55s применялся к native analysis с прежним max600s. Теперь native session message имеет610s, независимый от ASR deadline; loopback http.request исключает скрытый300s header timeout Node fetch. Новый delayed-native regression подтверждён RED при возврате старого deadline.
2. Jump и relay использовали один SSH control socket. Это обнаружено настоящим integration: reverse TCP был доступен, но health попал в jump и корректно отказал. У jump теперь ControlMaster=no/ControlPath=none, relay имеет собственный socket. Сквозной health подтверждён после исправления.
3. Обновление ключей/host pins могло оставить живой master на старом назначении. Candidate key проверяется до изменения рабочих файлов, обновление атомарно под marker, закрывается только собственный master, stale persisted config при новом target не запускается. Во время install health fail-closed. Negative rotations проверены без ручного restart.
4. Отмена split_audio во время ffmpeg могла оставлять приватные chunks. RAII guard владеет каталогом до cancellable await и передаёт ownership AudioChunk только после успеха. Genuine cancellation test создаёт private chunk и блокируется в дочернем процессе; abort удаляет каталог. Mutation без guard даёт AssertionError.
5. Browser выявил нестабильное вычисление label select после смены provider. Явные aria-label=field.label сохранены для controls; финальная actual Docker проверка проходит.

Открытых блокеров в проверенном локальном scope не обнаружено. Это не гарантия отсутствия дефектов вне проверенных сценариев.

## Проверки и evidence

- Финальный worker45/45 PASS:14 lib (включая реальную отмену подготовки аудио),8 HTTP,7 PostgreSQL pipeline,16 providers. Прочитан data/eval/phase14-worker-final.log. Сохраняются legacy OpenAI multipart/analysis OpenCode sessions, deny-all, cleanup, manual fields, versions, budget, purge и uncertain delivery. Основной protocol test проверяет оба exact ID, отдельные tenant credentials, timing/speaker/usage и отсутствие native chat для ASR.
- Финальный API/Actix/PostgreSQL3/3 PASS: новый asr_connector, processing_contract, recording_availability. Прочитан phase14-api-final.log. Прямой HTTP проверяет provider/base change без старого key, same-provider blank preservation, exact models, trusted URL, redaction, чужую организацию и readiness. Отдельная тестовая БД удалена; рабочие данные не подменялись.
- Node gateway17/17 PASS на host и внутри pinned hardened Docker. Прочитаны phase14-connector-node-green.log и phase14-connector-docker-fixture.log. Проверены protocol/auth/model allowlist, per-request keys, native boundary, invalid input/base64/size, safe response/usage/role filtering, errors/quota/redirects, concurrency, timeout/abort/shutdown и long-native deadline.
- Реальный pinned Docker metadata probe PASS (phase14-connector-docker-native.log): OpenCode1.18.10, native loopback4097/connector4096, оба exact config model ID, anonymous401, MCP0/toolsfalse, empty-session create/delete. Provider inference0. Контейнер nonroot/read-only/capdropALL/no-new-privileges, bounded memory/PIDs, без host ports или persistent app mounts.
- Python analyzer10/10 PASS: immutable init/pair/pins, protected paths, config injection/ports/project ownership, strict host pinning, candidate validation и credential reload, isolation от унаследованного MODEL_RUNNER_PASSWORD. Local и remote compose config --quiet PASS.
- Genuine Docker SSH PASS: data/eval/phase14-tunnel-gateway-smoke.json. Проверен remote-worker container на отдельной сети → model-runner Docker DNS → relay → reverse SSH через jump → actual local v1.18.10 connector. Health и каталог обеих ASR моделей получены через настоящий туннель. Nonroot10001/read-only/capdropALL, anonymous/arbitrary-command/wrong-client-key/wrong-host-key/occupied-forward-port denied, relay restart reconnect, mismatched pairing not healthy. HTTP fixture порты на host не публиковались. Fixture containers/networks/volumes и временные ключи удалены. Это реальный transport evidence, но не внешнее model inference.
- Frontend75/75 PASS, typecheck/Vite и финальный Docker client build PASS. Прочитан phase14-frontend-final.log, новый ASR selector/password label, legacy analysis, readonly/no mutation и secret switching проверены unit tests.
- Root actual Docker browser read-only PASS (phase14-browser-readonly.json): обе exact модели, отдельный tenant-key label, fixed readonly URL, mobile390 без overflow, legacy analysis free-text input, JS errors0, configuration writes0/model requests0. Собственная проверочная сессия завершена logout. Реальные настройки компании автоматически не менялись.
- Root сообщил build/deploy API/worker/client/model-runner healthy и успешный ./start-analyzer.sh local/status на собственном standalone project. SSH service к реальному серверу не запускался: target/keys/pairing ещё не получены. Это различие отражено в docs/local-analyzer.md.
- git diff --check PASS. Последний источник сборок — восстановленные исходники основной ветки. Один промежуточный stale Rust test artifact после mutation-source copy не использован как итоговый PASS/FAIL продукта; root принудительно перекомпилировал основную версию и повторил итоговый45/45.

## RED и ограничения тестового доказательства

Подтверждены:3 новых frontend tests RED на прежнем UI; основной worker STT protocol regression RED; direct API provider-key guard RED; RAII cancellation guard RED; connector baseline /asr/models RED на прежнем Docker. Отдельные connector mutations: runner auth, exact model allowlist, tenant-header/native boundary — AssertionError; native deadline mutation —504 вместо200. Tunnel mutations4/4: immutable pairing, jump mux, credential reload, invalid candidate preservation. Исходники основной ветки во время этих mutation проверок не заменялись.

Для остальных дополнительных guards отдельный RED не проверялся; GREEN coverage не следует выдавать за mutation proof каждого assertion. Точные команды/артефакты:14-SUMMARY-CONNECTOR.md,14-SUMMARY-TUNNEL.md,14-SUMMARY-FRONTEND.md, root backend summary/UAT. Приватные логи, JSON и изображения вне Git в data/eval; audio/телефоны/provider keys в Git не добавлялись.

## Открытая внешняя приёмка

Нужны адрес и отдельный разрешённый SSH-доступ к серверу Phone Platform, одноразовый deploy relay/pairing и доверенно сверенные host keys. В доступном Bondscan checkout нет актуального Docker source со скриншота: реализация переносит проверенную идею локального analyzer и создаёт независимый transport, не заявляет точное копирование неизвестной конфигурации. Bondscan .env/auth/SSH secrets/БД и процессы не использовались.

После pairing нужны собственный OpenRouter key компании, доступ к выбранным моделям и контрольный русский разговор для live проверки текста/участников/таймингов/стоимости. Платные реальные ASR/model запросы этой фазой не выполнялись. Веса ASR на ноутбук не скачиваются: provider inference идёт у OpenRouter. Компьютер/Docker должны оставаться включёнными. Финальный human UAT остаётся пользователю. Фаза12 не возобновлялась; её stash не применялся.
