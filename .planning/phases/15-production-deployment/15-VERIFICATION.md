# Phase15 — Independent implementation verification

Дата:2026-11-12. Verifier: `/root/deploy_plan_checker`, независимо от автора плана и deployment. Результат: **IMPLEMENTED / AUTOMATED VERIFIED**. **Human UAT / phase acceptance остаются pending**.

Проверка относится к фактическому `phone.bondscan.ru`, серверу `dks@81.16.141.170`, release `15-5789727`. Проверенный source commit `57897279c37c0a8dbeb129312111d347d8bd40f1` совпадал с origin/master на момент проверки. Последующие documentation-only commits не изменяют проверенный runtime release.

## Must-haves

| Требование плана | Результат / фактическое доказательство |
|---|---|
| Независимый публичный Phone с HTTPS | PASS: repeated HTTPS200, trusted chain/hostname, SANphone.bondscan.ru, expiry2027-02-10; HTTP301→HTTPS. Собственный nginx vhost→loopback18081, собственный сертификат/renewal hook. |
| Собственные данные компаний и доступы сохранены | PASS: migration baseline2org/6users/6memberships/27471documents/1247jobs/759calls/614recordings/10connections;614/614ready media verified, SHA256 архивов и document digest сверены до запуска writers;5encryptedconfigs реально AES-GCM decrypted без печати содержимого. |
| Публичные demo passwords отозваны | PASS: verifier повторил public smoke:3knownold401,3newlogins200, Secure/HttpOnly/SameSite=Lax; DEMO_MODEfalse. Access file private600, source users/orgIDs сохранены. |
| Tenant scope / media / assistant | PASS: anonymous calls401, mismatched org header409, foreignorgcall404, real Range206/64bytes. Assistant runnerready/authenticatedtrue через обычный API, без inference. |
| Реальный remote→local analyzer | PASS: verifier самостоятельно запустил отдельный remote Docker caller в Phone network: anonymous401/wrongpair401, authenticated nativehealth200/version1.18.10, exacttwoASRmodels200. Это настоящий relay→SSH→этоткомпьютер; no model inference. |
| Существующая история Git опубликована | PASS: origin=https://github.com/KirDontsov/phone-platform.git, master local==remote5789727 при проверке; история сохранена, phase12stashddddf25f4ef384b0a02359042efa4bee159a1f09 присутствует. |

## Независимые live checks

- Повторный `python3 scripts/smoke-production.py` прошёл все assertions после переключения runtime на release15-5789727. Проверены dashboard/connections/filter/recordingRange/tenant guards/auth status. Никаких settings writes, exports, processing starts или provider requests из smoke.
- `./start-analyzer.sh doctor`: configuration=true, ssh_http_roundtrip_and_pairing=true, local model-runner/tunnels оба running/healthy.
- Remote Docker ps: **7/7healthy** —db,rabbitmq,api,worker,client,assistant-runner,model-tunnel. Все5собственных application images имеют release15-5789727, linux/amd64; image digests зафиксированы в private `data/eval/phase15-release-images.txt`. Старые теги образов сохранены для rollback.
- Remote ports ss:18080/18081/25437/25672/18422 слушают только127.0.0.1;4096/4097 не слушают host. У relay опубликован только SSHloopback18422; runnerHTTP остаётся Dockerinternal.
- Docker inspect без Env: api/worker/assistant/relay readonly и nonroot; media/auth/relay-state — только ownphone-platform volumes, relay bind только ownPUBLICkey. Repo/hostSSHagent/Docker socket к AI runner не примонтированы.
- Remote stat: ownPUBLICpubkey644; `.env.platform`, analyzer-pair.env, production-migrated.json600. Host nginx error_logcrit действительно установлен; access format исключает query.
- Local api/worker/assistant-runner остановлены; cutover artifact подтверждает успешный finalize и сохранение локальной БД/volumes для rollback. Собственный исходный snapshot остаётся private. После production worker start calls стало762, исходные2org/6users/614recordings сохранены; это normal live import после baseline, а не подмена migration counts.
- HTTPSneighbors `lentaria.bondscan.ru` и `bondscan.ru` отвечают200, TLSverify0 после работ. Их runtime/env/DB/auth/privatekeys не читались.
- Public certificate дополнительно проверен Python ssl с OS trust bundle: CERT_REQUIRED/check_hostname=true, SANphone.bondscan.ru, validuntil2027-02-10. У python.org установки на этом Mac default CA store пустой; smoke явно подгружает системный `/etc/ssl/cert.pem`, не отключая TLS verification.
- Прочитан own Certbot dry-run log: simulated renewal success для phone.bondscan.ru. Это фактический dry-run, а не вывод о renewal из наличия timer.
- Independent Git all-ref sensitive-path audit:0 runtime/env/auth/key/audio/data/storage paths; `.env.example` — разрешённый template. Root также сообщил exact-known-secret scan564historyblobs с0matches; содержимое runtime не попало в verifier output.
- Rootdisk после deployment13GiB free, HDD1.9TiB free; BuildKit cache/output на собственном HDD пути, shared prune не выполнялся.

## Проверки кода и исправленных runtime findings

Все5findings из15-REVIEW.md исправлены: queryprivacy logs, writer hold through cutover, PUBLICkeyreadability дляUID10001, quoted tmpfs specs, preserved source modes/nonroot DockerCOPY. Начальная source/syntax проверка не обнаружила последние3ошибки; они выявлены настоящим container startup и после исправления проверены фактически.

- Независимый regression run **20/20PASS**, включая реальный PostgreSQL cleanup в новой disposable `phone_migration_test_*` БД; БД удалена в finally. Тесты сохраняют Codex generation/confirmation и connections, отзывают временные capabilities/sessions/grants, проверяют пустойtarget/privatefiles/checksum/upload guards/hold-restart/resume/Compose tmpfs parsing.
- Author RED-artifact `phase15-migration-red.json`:8/8negative mutations падают без соответствующих fixes. Verifier прочитал artifact, но самостоятельно эти8source mutations повторно не выполнял.
- Независимый `check-deploy-build.py` внутри настоящего LinuxDocker: immutable source contexts, preserved644/755 modes, private runtime600, failed staging cleanup, реальный concurrentflock PASS. Встроенный mode RED безtar-p воспроизведён verifier.
- Shell/Python parse checks PASS. Runtime relay compile/read probe выполнен verifier под UID10001; pinned Codex version в фактическом assistant image проверена без model request.
- Actual Chrome artifact `phase15-browser-smoke.json` прочитан: desktop1440/mobile390, login/dashboard/calls/settings, обеexactASRmodels, assistant authenticated, JSerrors0, mobileoverflow0, configurationwrites0/providerrequests0. Browser run выполнил root; verifier независимо повторил HTTP/login/media/tenant сценарии, не дублировал Chrome run.

## Evidence / границы

Приватные safe aggregate artifacts в ignored `data/eval/`: phase15-public-smoke.json, phase15-browser-smoke.json, phase15-live-tunnel.json, phase15-analyzer-doctor.json, phase15-migration.json, phase15-private-login.json, phase15-cutover.json, phase15-migration-red.json, phase15-web-renewal.log. Customer audio/transcripts/phone numbers/tokens не записаны в public planning docs.

**Pending отдельно от этой автоматической проверки:**

1. Финальный пользовательский UAT на публичном сайте. Accepted/completed phase count не увеличивать только из-за этого verifierPASS.
2. Собственный OpenRouter key выбранной компании, контрольная ASR запись и качество/стоимость реальных моделей. Codex login сохранён, но он не подключает OpenRouter ASR subscription. Платных inference calls в deployment verification0.
3. Независимый рабочий SMTP не предоставлен; invites/password reset не доставляются (явно unavailable127.0.0.1:1), Mailpit публично не запущен.
4. Live CRM/Google и пользовательская оценка качества прошлых фаз остаются их внешними gates. Фаза12 и её stash приостановлены, не включались в scope.

Компьютер и Docker должны оставаться включены для local analyzer. После его остановки веб-сервис/журнал остаются на сервере, обработка моделей ожидает восстановления tunnel/retry; healthytransport не доказывает доступ/качество upstream модели.
