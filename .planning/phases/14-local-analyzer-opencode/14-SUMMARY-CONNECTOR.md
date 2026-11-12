# Фаза14 — Docker OpenCode connector

Исполнитель: asr_research. PLAN-CHECK PASS0/3 прочитан до execute. Изменены только infra/model-runner и этот отчёт; Bondscan и работающие контейнеры основной платформы не менялись.

## Реализация

- Native OpenCode закреплён1.18.10 и слушает только loopback4097. Публичный внутри Docker connector4096 проверяет Basic runner password; прежние GET health/config/provider/tool metadata и создание/удаление session работают через ограниченный proxy.
- В opencode.json добавлены точные moondream/parakeet-ultra и nvidia/nemotron-3.5-transcribe под provider.openrouter.models. Конфигурация проверена настоящим pinned бинарником. Metadata audio→text, tools/reasoning false; вымышленные token limits не добавлялись. Default текстовой модели не менялся; MCP0/tool-deny/plugin0 сохранены.
- Новые GET /asr/models, POST /asr/test, POST /asr/transcriptions — явное расширение connector, не имитация native OpenCode chat inference. STT отправляется на фиксированный https://openrouter.ai/api/v1/audio/transcriptions, JSON input_audio/rawbase64; /asr/test проверяет собственный tenant key через /api/v1/key без inference.
- Transport runner password и OpenRouter key компании разделены. Последний поступает на каждый ASR request в X-OpenRouter-Api-Key, не сохраняется в env/config/cache, не пересылается в native OpenCode. Общего fallback provider key нет. Unknown model/URL/control/body fields запрещены до upstream upload. Внешние redirects запрещены; код не принимает upstream URL из config/env.
- Auth до bodyread, max2 ASR одновременно с gate до чтения тела, encoded request34MiB/decoded audio24MiB, response8MiB, ASR deadline55s. Client disconnect/shutdown прерывают fetch. Сырые upstream ошибки и account metadata не возвращаются; static code/message и bounded Retry-After. Text/segments/words/usage проходят allowlist; numeric speaker labels сохраняются, manager/client roles не доверяются.
- Закрыт P1 независимого reviewer: короткий ASR deadline не обрывает прежний анализ. Native session message имеет610s deadline для прежнего worker max600s; используется signal-bounded loopback http.request, исключая скрытый300s header limit global fetch. Metadata по-прежнему ограничена коротким deadline. Stdout/stderr native child не транслируются в обычные логи; startup/exit дают статические коды.

## Проверки

- Node HTTP fixtures17/17 PASS: обе модели, отдельные tenant keys, правильный JSON/STT endpoint, test без inference, catalog/auth, unknownmodel/controls, неверный base64/format/options, native compatibility/credential boundary, auth before huge body, decoded/encoded limits, quota/error redaction, redirects/no retry, response size/shape, forged roles, concurrency, timeout, disconnect/shutdown, независимый long-native deadline.
- Baseline RED на прежнем настоящем Docker: /asr/models вместо JSON каталога возвращал native UI HTML; новая assertion это обнаруживает. Дополнительные3 RED mutations на копии исходников: bypass runnerauth, bypass exactmodelallowlist, remove tenant-header/nativeboundary. Все завершились assertion failure, исходники основной ветки не менялись. Отдельный RED long-native deadline: вернуть ASR deadline для native → новый тест падает504 вместо200. Для остальных guard tests отдельный RED не проверялся.
- Собран собственный isolated image phone-platform-model-runner:phase14-probe. Реальный pinned Docker health/config/provider/tool metadata и empty session create/delete PASS: version1.18.10, public4096/native4097, два exactID в настоящем config, anonymous401, MCP0, toolsfalse, registeredtools14, tenantkeyrequired. Model/provider inference requests0.
- Внутри того же hardened Docker повторены все17 HTTP fixtures PASS. Контейнер non-root10001:10001, read-only root, capdropALL, no-new-privileges, memory/pid limits, без host ports и persistent/bind mounts; использованы только tmpfs. После проверки isolated контейнер удалён; исходные сервисы не останавливались.
- git diff --check и node --check gateway/start PASS.

Приватные артефакты: data/eval/phase14-connector-baseline-red.log, phase14-connector-red-summary.json, phase14-connector-native-deadline-red.log, phase14-connector-node-green.log, phase14-connector-build.log, phase14-connector-docker-native.log, phase14-connector-docker-fixture.log. Probe orchestration data/eval/phase14-connector-docker-probe.py использует только synthetic password и fixtures, не реальные данные.

## Остаётся отдельной приёмкой

Production remote SSH tunnel/worker integration выполняется другими блоками фазы. Доступ конкретного OpenRouter аккаунта, фактическая поддержка verbose timestamps/diarization каждым endpoint и качество на русских customer recordings не проверены живыми model calls. Unsupported verbose_json не заменяется скрытой второй платной попыткой. OpenCode/Codex подписка не подменяет OpenRouter credentials. Фаза12 остаётся приостановлена.
