# Phase19 — независимая проверка плана

Дата:2026-11-13. Проверяющий:routerai_checker; автор:root.

**Результат: PASS, revision1/3.** Структура GSD valid=true,3tasks, errors/warnings=[]; research соответствует уточнению пользователя и границам фазы18. Это проверка плана, не реализации/внешнего аккаунта.

Проверены AGENTS/STATE/PROJECT/ROADMAP,19-RESEARCH/19-01-PLAN, контракт local-models-v1 и существующий gateway. Секреты/учётные записи/production не читались и не менялись. Данные публичного каталога и отсутствие локальныхcredentials принимаются как явно отмеченное evidence автора, не как собственная live-проверка.

В revision1 внесены и повторно проверены три уточнения:

1. **Однозначная маршрутизация RouterAI против OpenRouter.** Оба используют те же `moondream/parakeet-ultra` и `nvidia/nemotron-3.5-transcribe`. Новый маршрут должен явно получать выбранный источник/local_provider от проверенного serverconfig/job, а не угадывать его по modelID или отсутствию API-key. Сочетание local selector +legacy key отвергается; пропущенный/некорректный source не переключается на другой провайдер. Regression: одинаковый modelID localRouterAI и legacyOpenRouter вызывает разные фиксированные endpoints с соответствующим credential; отсутствующий/смешанный выбор не вызывает upstream.
2. **Стоимость в RUB не смешивается с общей cost.** `usage.cost` RouterAI содержит рубли; простого добавления currency в JSON недостаточно, если существующие aggregates/budget продолжают складывать bare cost. Допустимый минимальный вариант: сохранять исходные `provider_cost`/`provider_currency:RUB`, оставлять общий `cost:null`, пока потребители не поддерживают валюты. Либо сделать их currency-aware в этом scope и проверить. Нельзя заявлять USD или применять RUB как бюджетUSD.
3. **PKCE callback и runtime commit.** Помимо state/issuer/path/TTL проверить точный loopback Host и отвергать повторяющиеся code/state/iss параметры; некорректный callback не расходует корректную попытку. Одноразовый обмен помечается до сети, в том числе при timeout, и не повторяется. После consent runtime сохраняется под filelock с повторным чтением и tenantcheck, merge с текущими providers, атомарным0600write; stale prebrowser snapshot не должен стирать параллельно добавленные настройки или перепривязывать company. Ошибка exchange/install сохраняет прежний runtime; неопределённый exchange не повторяется тем же code.

Остальной scope приемлем: существующий RouterAIаккаунт через явное browserconsent, собственный Phone app_name/org, нет копирования Bondscan/создания покупок/операторского платногоASR, отдельная UAT. UI без ручного ключа и явный выбормодели, tenant/sessionguards и denyconfig фазы18 сохраняются. Safe lease-gated rollout worker и сохранение Telphinbackoff предусмотрены. Отсутствие credentials не заменяется fixtures.

Чеклист Григория: не проверял, нет чеклиста.

## Повторная проверка revision1

Автор согласовал точные решения. PLAN дополнен: explicit local_provider и failclosed mixed/missing source; RouterAI RUB в provider_cost/provider_currency отдельно от commoncost/budgets; callback Host/querydup/state/issuer/oneshot до сети и locked current-file merge/tenantrecheck/atomic600commit. Проверка обновлённого текста подтверждает все три gate. **Блокеров execute нет.** Результат остаётся проверкой плана; реализация/helper/внешняя авторизация ещё не проверены.
