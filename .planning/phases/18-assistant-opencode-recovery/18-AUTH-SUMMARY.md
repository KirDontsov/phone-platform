# Phase18 — CONNECT / Codex auth preservation

Исполнитель: assistant_auth_diagnosis. Изменения подготовлены локально, production/учётные записи не изменялись, коммиты выполняет root.

## Реализация

- `infra/model-runner/codex-proxy.mjs`: `attachCodexProxy(server,{password})` возвращает close()/activeCount; Basic pairing opencode, только три точных OpenAI hostname:443, отсутствие DNS для запрещённой цели/auth, все DNS-ответы проверяются на публичность, выбранный IP фиксируется, преимущество IPv4, нет повторного DNS при соединении. Не разрешены IP-literal/private/mixed DNS/другие порты/userinfo. Ограничены DNS/connect/idle/lifetime, максимум4 active CONNECT; shutdown уничтожает свои sockets. Токены и трафик не журналируются.
- Assistant `childEnvironment` получает только фиксированный paired HTTP(S)_PROXY model-runner:4096 и фиксированный NO_PROXY для internal API/loopback. Host proxy/OpenAI key/DBenv не наследуются. Remote overlay добавляет только собственный pairing и адрес.
- Node/API разделяют active и pending generation. Failed new login не удаляет прежний credential, не снимает authenticated/confirmed_generation и не отменяет действующие runs.
- Auth callback поддерживает prepare/commit/status, default absent phase=legacy commit. Prepare проверяет исходную живую session/member/owner/capability/TTL, commit активирует новую generation и отзывает прежние runs. Cleanup включает prepared/committed attempts.
- До promotion записывается durable authenticated=false/commit_uncertain marker и private600 backup прежнего auth. После restart новый credential не исполняется под старой confirmed generation. Commit retries ограничены; status receipt разрешает lost-response case. Старый файл восстанавливается только при достоверном receipt прежней active generation. Неразрешённая неопределённость сохраняет оба private файла и блокирует turns. Explicit logout/cancel удаляет оба файла и отзывает pending/active.
- Контракт обновлён в `docs/contracts/assistant-v1.md`. Не объявляется атомарность распределённой операции между DB и runtime filesystem.

## Проверки

- `phase18-assistant-all.log`: **26/26 PASS**, в том числе3 настоящих pinned Codex/MCP offline-probes, failed reauth сохраняет credential/generation/старый turn, pending/logout, lost commit receipt, unresolved uncertainty, actual createRunner restart после promotion и после commit до state write.
- `phase18-proxy-green.log`: **4/4 PASS**, настоящие loopback TCP opaque TLS bytes, auth/target/DNS guard, bounded DNS/concurrency/close.
- `phase18-api-auth-green.log`: **1 API state-machine unit PASS**.
- `phase18-api-auth-pg-green.log`: **1 настоящий PostgreSQL/HTTP regression PASS**, 133.49s. Failed runner auth оставляет API authority и старый MCP run рабочими; prepare не меняет confirmed generation; commit отменяет старый run; logout отклоняет late pending callback.
- `phase18-api-auth-existing-pg.log`: **существующий stale-generation/revoked-session PostgreSQL test PASS**, 93.10s. Использован только отдельный root fixture, не production DB; большая длительность обусловлена множеством запросов через SSH.
- RED подтверждён: baseline failed-reauth Node regression (`false != true`); baseline child env не передавал proxy; изолированные mutation-копии proxy без auth guard и без private-DNS guard проваливают новые tests. Отдельная RED для каждого нового assert/API PG case не выполнялась.
- `node --check` и `git diff --check` PASS. Платных/model requests не было: native probes используют настоящий CLI, но изолированный offline HTTP fixture.

## Что остаётся root

Независимое review, gateway/Docker интеграция export, build/release и настоящий isolated device challenge через production→local CONNECT. Human OAuth требует личного завершения пользователем. Provider/model catalog/ASR и tenant-bound import выполняются root/другими исполнителями. Phase17 worker/checkpoint/backoff не менялись этим исполнителем.
