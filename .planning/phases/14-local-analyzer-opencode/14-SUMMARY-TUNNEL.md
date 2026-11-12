# Фаза14 — portable local analyzer и SSH transport

PLAN-CHECK PASS0/3 предшествовал исполнению. Реализованы только Phone Platform файлы. Bondscan, его процессы, .env/auth/ключи/БД не использовались и не менялись. Публичный remote SSH target ещё не получен: живое подключение к серверу не объявляется готовым.

## Результат

- `./start-analyzer.sh` — повседневный запуск собственного standalone Compose project `phone-analyzer`; запускает local OpenCode connector и rootless SSH client. Remote worker продолжает хранить БД/очередь/медиа; HTTP вызов проходит через private Docker relay к local connector.
- `init` создаёт только Git-ignored Phone `.env.analyzer.local`, `secrets/analyzer-pair.env`, отдельную ed25519 пару и known_hosts. Повторный init не заменяет существующие credentials/pins. Приватные файлы0600, собственный SSH каталог0700; symlink paths отклоняются. Main `.env.platform` не читается и не переписывается этим helper.
- `local` запускает только connector для локальной проверки, без объявления SSH готовым. `status` выводит только service/state/health; `doctor` проверяет конфигурацию, собственные pins и настоящий сквозной HTTP/пароль. `public-key`, `pin --jump-public-key/--relay-public-key`, `remote-instructions` покрывают однократное pairing. `down` останавливает/удаляет только два контейнера собственного project, не глобальные SSH процессы/чужие сервисы и не volumes.
- Локальный compose независим от текущего `phone-platform`, публикует connector только на127.0.0.1:18496. Ни DB, ни media, ни Docker socket/host home/SSH agent не монтируются.
- Remote overlay поднимает `model-tunnel` с Docker alias `model-runner`. SSH опубликован только наremote127.0.0.1:18422; reverseHTTP4096 слушает только внутри Docker network и не публикуется на host. Remote API/worker получают одинаковый paired MODEL_RUNNER_PASSWORD, relay не блокирует запуск сайта ожиданием локального компьютера.
- Local client выполняет ProxyJump через explicit Phone user/key и strict pinned host keys, затем `-R0.0.0.0:4096:model-runner:4096`. `ExitOnForwardFailure`, keepalive и bounded reconnect; родительские SSH config/agent не используются. Docker exec stdin передаёт собственный ключ в отдельный rootless volume, что избегает небезопасных host bind/UID предположений Mac.
- Dedicated OpenSSH server и client проверены с uid10001, read-only root, cap_dropALL/no-new-privileges. Relay разрешает только reverse4096; TTY/agent/local/unix forwarding и произвольные command sessions запрещены. Единственный forced health program проверяет native connector через reverseHTTP с paired password; control socket сам по себе не считается готовностью.
- При обновлении credentials сначала валидируется candidate key, затем файлы заменяются и завершается только собственный relay master. Persisted config старого назначения не используется при смене environment/target. У jump host multiplexing отключён, relay control socket отдельный. Во время установки health fail-closed.
- Pairing-file password явно перекрывает случайно унаследованный MODEL_RUNNER_PASSWORD другого project. Remote-инструкция также исключает env override. Автоматического deploy/изменения remote host sshd нет.

## Проверки

- `python3 -m unittest scripts.tests.test_analyzer`:10/10 PASS. Immutable init, config injection/пустой host/порты/чужой project, changed host pin, symlink/permissions, strict SSH config, candidate-key failure preserves previous credentials, own-master reload, inherited-env isolation.
-4 мутационных RED на отдельной копии исходников, без изменения shared source: убрать immutable pairing, вернуть общий jump/relay control socket, убрать own-master restart при credential update, убрать предварительную проверку private key. Все4 упали по AssertionError, не по import/build/setup. Артефакты `data/eval/phase14-tunnel-red-summary.json` и соответствующие logs. Для остальных unit guard cases RED отдельно не проверялся.
- Genuine SSH integration сначала выявила ошибку общего multiplex socket: reverseTCP и remote-direct HTTP были готовы, но local health обращался к jump и корректно возвращал failure. После исправления полноценная цепочка проходит.
- `docker compose … config --quiet` для standalone local и remote overlay PASS. Tunnel Docker image build PASS.
- `python3 scripts/smoke-analyzer-tunnel.py --runner-image phone-platform-model-runner:phase14-probe --require-asr --report data/eval/phase14-tunnel-gateway-smoke.json`:PASS. Настоящие2 SSH hops, actual pinned OpenCode1.18.10 gateway, отдельный remote-worker контейнер обращается по Docker DNS через relay→reverseSSH→local connector, `/global/health` и `/asr/models` с обоими exactIDs.
- В этом же Docker тесте подтверждены nonroot/readonly/capdrop,401 без auth, отказ arbitrarycommand, автоматическое восстановление после restart relay, отказ wrong host/client keys с восстановлением **без ручного restart client**, отказ занятого reverseport и fail-closed при несовпадающем pairing password. Ни один HTTP порт fixture не опубликован на host. Только metadata; model inference0, реального аудио/номеров нет.
- Все fixture containers/networks/volumes и временные ключи удалены после проверки. Main platform/чужие контейнеры не трогались. `git diff --check` и Python compile PASS.

## Однократное подключение настоящего сервера

Нужны actual server host/user, отдельный Phone public key в ограниченном authorized_keys jump-user, размещение публичного client key + защищённого pairing file на сервере, remote overlay и доверенно сверенные host keys. Приватный client key остаётся на этом компьютере. Актуального Docker source со скриншота в checkout нет; это независимый Phone transport по требуемой схеме, а не утверждение о побайтовом переносе неизвестной конфигурации.

Parent/root выполняет запуск standalone `local` на этом компьютере, worker/API/frontend интеграцию и финальный independent verifier. Live remote host и live OpenRouter ASR качество/стоимость остаются отдельными gates.
