# Phase18 — независимое review root components

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker; автор root components — root, UI — opencode_settings_ui.

**Результат: PASS по рассмотренным компонентам после исправлений.** AuthService/API generation staging рассматриваются отдельно после окончания работы auth agent; production/native smoke и внешний пользовательский OAuth этим review не подтверждаются.

## Исправленные замечания

1. **P1: перенос внутреннего pairing во внешний analysis endpoint.** Исходный save_connection_in_db проверял fresh key только для ASR; при смене analysis local OpenCode на внешний endpoint сохранялся старый общий transport api_key. Сейчас validate_model_key_transition вызывается и для analysis: изменение provider/destination без свежего внешнего ключа отклоняется, local pairing инъецируется только сервером. Actual helper используется save path; независимый unit test PASS. Позднее просмотрен настоящий PostgreSQL/Actix save test root: keyless local ASR, orgspoof override, pairing server injection/canonical slash, externaldestination rejection/freshkey PASS за93.74s; guard mutation без исправления даёт HTTP200 вместо ожидаемого400.
2. **P2: session capacity после upstream create.** Независимая synthetic loopback-проба воспроизвела129 upstream creates при лимите128 и последнем502. Исправлено reservation перед nativeRequest с учётом concurrent creates и освобождением finally; тест maxSessions=1 подтверждает, что второе создание не достигает upstream, после DELETE ёмкость восстанавливается.
3. **P2: нормализация local analysis URL.** Guard распознавал trailing slash, но pairing инъецировался только при literal URL. Условие инъекции теперь использует такую же normalized_base, сохраняет canonical URL и всегда подставляет серверный pairing. Некорректные/внешние URL не становятся local.

## Рассмотренные контракты — PASS

- `gateway.mjs`, `local-providers.mjs`, `start.mjs`, importer и Compose volumes: импортируются только доверенные provider definitions/keys в собственный private600 runtime, фиксированный compatible SDK, без host tools/plugins/MCP/auth. Native deny config сохраняется. Ключи не пересылаются на remote server/browser; в native JSON они редактируются как по названиям полей, так и по точному значению.
- Wrong/missing trusted organization закрывает все разрешённые native config/provider/tools/session execution paths. Session map связывает ID с организацией; чужая/неизвестная session и незаявленная модель отклоняются до native call. Public doc/health не исполняют модели; всё равно проходят redaction. Generic proxy не разрешает произвольные маршруты или изменение config.
- API ctx и worker job сами задают `_organization_id`; caller value не переопределяет tenant. API catalog восстанавливается из public DTO fields. Fixed local transport credentials отделены от глобальных provider credentials и внешнего OpenRouter key.
- APInexVoiceSTT — отдельный честный ID/label и opt-in credential_source=local; исходные два OpenRouter IDs сохранены. Платные STT/model calls не запускаются загрузкой каталога или сохранением UI. Metadata probe только GET/models; это проверка доступа, а не доказательство качества/оплаты/успешной ASR. Счётчик внутренних APInex units не выдаётся за dollar cost; роли не угадываются.
- Фронт скрывает pairing для local analysis, предоставляет явный scoped model selector; local ASR не отправляет api_key, legacy OpenRouter остаётся отдельным режимом. Existing конфиги не меняются автоматически. Full95/UI typecheck/build и5RED evidence просмотрены, не выдаются за production browser test.
- Сохраняются ограниченные request/response bodies и deadlines, provider redirects не следуются, raw upstream errors не возвращаются. Native async message читает JSON с отдельным длинным deadline; streaming не добавлялся. Model prompt остаётся недоверенным и не получает tools/MCP.

## Проверки

Независимо выполнено `node --test infra/model-runner/tests/{gateway,local-providers,codex-proxy}.test.mjs`:25/25 PASS. Дополнительно `cargo test -p phone-api --lib local_pairing_never_follows_an_analysis_destination_change`:1/1 PASS. Изначальная ошибка нового теста role исправлена корректно: gateway strips provider role, downstream определяет unknown, а не доверяет присланной роли.

Просмотрены worker/provider/model regression и4 importer tests evidence root. Собственные внешние provider/model вызовы и production mutations проверяющим не выполнялись, секретные runtime files не читались. Для capacity bug независимый RED действительно воспроизведён; unit pairing guard проверен GREEN, RED-мутацию этого guard проверяющий отдельно не запускал. Остальные RED и настоящие PG/prod gates автор фиксирует в общем VERIFICATION после завершения.

Дополнительно рассмотрен processing.rs: keyless local ASR исключает api_key из обязательных полей, но сохраняет validate_config с local_provider/model и серверным org. Просмотрен actual PG RED этого preflight (без исправления ready=false вместоtrue) и GREEN той же HTTP/PG suite. Это просмотр evidence автора; production preflight отдельным вызовом проверяющим не менялся.

Соответствие плану18-01 по root components — да. Чеклист Григория — не проверял: нет чеклиста.
