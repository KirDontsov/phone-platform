# Phase19 — независимое review реализации

Дата:2026-11-13. Проверяющий:routerai_checker; реализация:root/routerai_ui/helperagent.

**Результат: PASS. Блокирующих замечаний нет.** Это code/testreview, не подтверждение личного аккаунта RouterAI или настоящего распознавания.

## Проверенный scope

- Рассмотрены gateway/local-providers, worker asr_connector/models, private importer, SettingsPage/UItests, новый connect-routerai.py и его HTTPfixtures. Проверяющий не читал production/секреты/чужие аккаунты и не менял их.
- Идентичные ASR modelID двух провайдеров не смешиваются: local_routerai передаётся явно через `local_provider`, tenant проверяется до upstream, фиксированный RouterAIhost и собственный runtimekey используются только в этом режиме. Mixed local+legacykey, чужая компания, недопустимые модели и пропущенный legacykey не вызывают RouterAI fallback. LegacyOpenRouter продолжает использовать свой host/key. Модели речи не появляются в native text choices и отвергаются native message gate.
- Source/runtime/routerai endpoint нельзя изменить на чужой host. Generic native config/provider/session guards и redaction фазы18 сохраняются; пользовательский config не определяет organization_id. Localcatalog ничего не сохраняет и не запускает модели. UI берёт local_provider из выбранной catalogentry, показывает RouterAIhint вместо APInex и требует явный выбор/сохранение; те же ASRmodelID в legacyрежиме остаются отдельно.
- RouterAI STT стоимость в RUB сохраняется отдельно в provider_cost/provider_currency, общий cost остаётсяunknown/null даже при старом ответе connector с rawcost. Chunkaggregation суммирует только подтверждённые RUB amounts, неизвестный chunk делает итогunknown. NativeRouterAItext не выдаёт SDK computedcost за подтверждённую стоимость: commoncost=null. Прежние aggregate/budget totals не получают рубли или фиктивный free0.

## OAuth/helper

- Собственный app_name содержит Phone/org и не перезаписывает приложение OpenCode/Bondscan. Loopbackserver ephemeralport, exactHost/path/state/issuer/TTL, duplicatequery/headers rejection; неверный callback не расходует корректную попытку. Callback query немедленно уходит в чистый /connected, accesslogs подавлены, webbody/ошибки не раскрывают code/verifier/key. AuthURL содержит только публичный challenge/state, не verifier.
- Exchange destination фиксирован, TLS проверяется, redirects запрещены, тело ответа ограничено. consumed и exchange_started выставляются до сети; timeout/replay/ручное повторное finish не обменивают прежний code снова. Это необходимо из-за семантики отзыва ключа при повторном exchange. Пользовательский OAuthconsent не имитируется и не заменяется syntheticcredentials.
- Runtimecommit использует общий с importer filelock, заново читает текущую private600копию и tenantbinding, merge сохраняет параллельно добавленные providers. Atomicrename/privatewrite и rollback текущего volume snapshot при install/hostwrite error проверены. При неопределённом rollback сообщаетсяuncertain, без повторного exchange. Restart ограничен собственным phone-*-model-runner и ждёт health; DockerSocket в runtime не добавлялся.

## Замечания review и исправления

1. Importmain раньше писал новый hostfile до Dockerinstall и при ошибке оставлял разные поколения host/volume. Исправлен порядок: под lock read-current→prepare600temp→install→replace; при ошибке rollback точного current snapshot, прежний hostfile сохраняется. Два actual-main fixture tests покрывают failure и сохранение приватного RouterAI при последующем globalimport.
2. NativeRouterAItext раньше доверял SDK info.cost без подтверждённой currency/pricing. Теперь commoncost=null; targeted actualHTTP regression проверяет и значение3.25, и сохранение токенов/cleanup. Автор подтвердил RED, проверяющий самостоятельно rerunGREEN.
3. RouterAI `/key` metadata теперь требует objectdata, а не произвольный HTTP200object; malformedauthmetadata не объявляет настроенный аккаунт. Проверка accountpermission реальным ключом остаётся UAT.

## Независимая проверка

- Node gateway/proxy/local-provider/RouterAI suites: **30/30 PASS**. Среди реальных loopbackHTTPfixtures same-model route/key separation, wrongtenant/mixedkeys/no-fallback, RUB isolation, metadata-only auth/modelprobe; provider requests только synthetic.
- Python connect-helper/import suites: **18/18 PASS**, включая настоящий localhost callback и HTTPproviderfixtures, one-shot timeout/replay, redirects, wrongstate/issuer/Host, текущий tenant и атомарный merge/install rollback.
- `cargo test -p phone-worker --test model_http routerai_ -- --nocapture`: **2/2 PASS**, оба исходных STTID/headers/keyless/RUB и native text unknowncost. Новые UItests/RED/evidence изучены; сам полный frontend здесь не повторял.
- Отдельные RED каждого assert проверяющим не выполнялись. Авторские RED UI/helper/security/currency фиксируются в phase19 evidence/summary; liveaccount/ASRquality этим не подтверждаются.

Соответствие плану19-01 revision1: да. Перед выпуском остаются nativebuild, safe idlelease rollout, readonlyproduction/verifier и человеческий RouterAIOAuth при отсутствии местных credentials. Существующие Telphin jobs/backoff, Codex auth, SMTP и phase12stash не должны сбрасываться. Чеклист Григория: не проверял, нет чеклиста.
