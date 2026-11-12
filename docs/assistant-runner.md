# Codex assistant runner

Фаза 11 добавляет отдельный `assistant-runner:8091` к стандартному Compose. Его запуск входит в `scripts/dev.sh up`; порт не публикуется на хост. Существующий OpenCode analysis runner остаётся отдельным сервисом с MCP0. Bondscan, host auth.json, домашние каталоги и его credentials не используются.

## Runtime и доступы

Закреплены `@openai/codex@0.167.2`, `@modelcontextprotocol/sdk@1.32.2`, Node24 и lockfile. `setup-local.py` создаёт/добавляет только недостающие `ASSISTANT_RUNNER_TOKEN` и `ASSISTANT_RUNNER_URL` в приватную `.env.platform`, сохраняя существующие значения. Shared token получает только API и управляющий Node-процесс. Codex/MCP children не наследуют DB, RabbitMQ, encryption key, shared runtime token или host environment.

В контейнере только source runner/SDK/CLI, отдельный auth volume и tmpfs. Nonroot10001, read-only root, cap_drop ALL, no-new-privileges, memory1GiB/pids128; Docker socket, репозиторий, media и пользовательский home не монтируются. UID auth volume наследуется от каталога образа; директории0700, файлы0600. Auth volume **не входит в обычный backup**; после restore API требует новую авторизацию, runtime generation сама по себе не включает доступ.

Через защищённые owner endpoints доступны API key и ChatGPT device code. Ключ передаётся Codex только по stdio и сохраняется его собственным file-auth store; не входит в chat/model/history/audit. Device verification URL ограничен `https://auth.openai.com/codex/device`; pending code виден только исходной сессии на стороне API. `expires_at` — собственный deadline runner10min, не выдуманное поле Codex protocol.

Авторизация сначала staged в отдельном temp home. Перед canonical promotion Node вызывает `POST /api/assistant/auth/complete` и требует ровно `data.approved=true` плюс совпадающую generation. API повторно проверяет original session/membership/owner/TTL/generation. Никакое2xx/наличие файла/model-list не подменяет эту проверку. Mutex и generation защищают от позднего callback/refresh после cancel/logout/relogin. API создаёт generation выше DB, runtime и текущего timestamp; stale status не является авторизацией.

## Граница возможностей

Каждый turn получает отдельные process/home/workdir и новый ephemeral thread. История приложения передаётся ограниченным JSON заново; чужие threads не возобновляются. В обоих `thread/start` и `turn/start` установлены `environments=[]`, `runtimeWorkspaceRoots=[]`, read-only sandbox/network=false. Config выключает shell/web/images/browser/computer/agents/apps/plugins/hooks/memories/code-mode/skills. Дополнительно `history.persistence=none`, memory generate/use=false, project docs0, daemon auto-start=false. Старт — `--strict-config`; obsolete настройки не игнорируются.

Runner не предоставляет JSON-RPC passthrough. Разрешён минимальный fixed client-method set для handshake/auth/config/thread/turn. `thread/shellCommand` в native protocol работает unsandboxed: он, `command/exec`, environment registration, config writes/imports не разрешены wrapper-ом. Все server-initiated запросы отклоняются; raw stdout/stderr, reasoning, args и tool outputs не проксируются в продукт. Неизвестный/запрещённый item/event завершает run безопасной ошибкой. NDJSON имеет один terminal event, bounded Unicode text и no-store; EOF не равен completed.

Единственный MCP server — реальный официальный SDK stdio bridge. Он знает8фиксированных names по [контракту](contracts/assistant-v1.md); каталог проверяет exact allowlist, схемы валидирует SDK AJV. Gateway — фиксированный `POST /api/assistant/tools`; ни URI, ни RPC, ни компанию/сессию модель не выбирает. Capability живёт только в runtime env MCP child, не в prompt/argv/results. На каждом вызове API проверяет исходную власть, revision и receipt. Call ID генерируется bridge один раз на invocation. Даже unknown/schema errors расходуют его лимит24invocations; сервер также ограничивает durable budget.

Для этих8typed tools задан per-tool `approval_mode="approve"`: иначе native `approval_policy="never"` отклоняет MCP writes даже при `auto`. Это не разрешение OS-команд: native environment capabilities отсутствуют, а bridge имеет только типизированные API-операции. Native каталог может содержать `request_user_input` и MCP resource helper functions; user-input RPC отклоняется, а bridge не реализует resource handlers и не умеет читать file URI. Доказательство отсутствия OS exec опирается на реальный negative test, не только prompt или флаг.

## Отмена и очистка

По cancel/disconnect/deadline180s/logout процессная группа получает TERM/KILL; MCP descendants также завершаются. Temp dirs удаляются в finally, при старте собираются остатки после crash. Pending auth имеет timer; signals ожидают закрытия активных процессов. Уже committed настройки отмена не откатывает; API receipt показывает результат. Native cleanup/revocation не заменяют API capability checks.

## Проверки

```sh
npm --prefix infra/assistant-runner ci --ignore-scripts
npm --prefix infra/assistant-runner test
docker compose --env-file .env.platform -f infra/compose.yml up -d --build --wait assistant-runner
docker compose --env-file .env.platform -f infra/compose.yml exec -T assistant-runner npm test
docker compose --env-file .env.platform -f infra/compose.yml exec -T assistant-runner npm run smoke
```

15 Node tests локально PASS: HTTP token/explicit missing auth, canonical promotion approval, generation/tenant fencing, device TTL/completion/logout cleanup, Unicode single-terminal/redaction, EOF/forbidden/unknown/user-input denial, cancel и HTTP disconnect с завершением процесса, настоящий SDK stdio list/call/schema/budget. Native test использует **настоящий Codex**, настоящий bridge и loopback Responses fixture: разрешённый MCP call достигает API; forced `exec_command` получает `unsupported call`, marker отсутствует. Настройки совпадают с production кроме локального фиктивного upstream provider; внешних model requests0. Это не имитация app-server и не runtime fallback.

Настоящие device approval, model entitlement/quality/usage и разрешённая проверка Телфина остаются live acceptance после настройки владельцем. Без auth платформа показывает необходимость подключения и не отвечает синтетическим текстом от имени модели.

Официальные источники: [app-server](https://learn.chatgpt.com/docs/app-server.md), [MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [config](https://learn.chatgpt.com/docs/config-file/config-reference). Поведение закреплено фактическими protocol/config/negative тестами версии0.167.2.

Фактический Docker PASS:15/15 tests, service healthy, anonymous control401, fresh-org authenticated=false/generation0, actual native+SDK negative probe PASS. Docker inspect подтвердил UID10001/read-only root/cap-drop/no-new-privileges/limits/no host ports/no host binds; единственный volume — auth, DB/encryption/broker/model-runner secrets отсутствуют. Приватный машинный отчёт: `data/eval/assistant-runner-smoke.json`. API/browser/live acceptance этой проверкой не подменяется.
