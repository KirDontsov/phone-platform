# Помощник настройки: UX и frontend research

Дата: 2026-11-12. Фаза11. Статус: research/UX согласован с draft `docs/contracts/assistant-v1.md` и планом11-03; реализация ещё не начата, ожидается plan-check PASS. Wire format закреплён контрактом; ниже UI-представление этих данных.

## Что взято из Bondscan

Изучены только исходники, без БД, env и учётных данных соседнего проекта:

- `../obligation-platform/obligation-client/src/features/ai-chat/ui/AiChatFab.vue`: глобальная FAB, drawer520px, Escape/close, mobile переход на отдельную страницу, скрытие FAB на странице чата.
- `AiChatPanel.vue`, `ConversationSidebar.vue`: один общий panel для страницы и drawer; список диалогов, создание/переименование/удаление; на узком экране выбор между историей и перепиской.
- `ChatComposer.vue`, `ChatMessageList.vue`, `ToolUseChips.vue`: отдельные send/stop, подсказки пустого состояния, bubbles, состояния thinking/error/cancelled, нейтральные chips инструментов.
- `src/entities/ai-chat/model.ts`, `src/shared/api/ai-chat.ts`, `src/shared/lib/ai-chat-sse.ts`: AbortController, in-flight state и поток через fetch.

Для нового сервиса используем его собственные teal/light стили и тексты. Premium/paywall, финансовые инструменты, Ollama warmup и имитация печатания готового ответа не нужны. Историю действий инструментов сохраняем на сервере: в образце chips существуют лишь в памяти последнего turn. Закрытие HTTP-потока само по себе не доказывает успешное завершение изменений.

## Вход и компоновка

Помощник настройки доступен владельцу. Manager/reviewer не получают FAB/редактирование; сервер независимо проверяет роль на чтение истории, запуски, credentials и tools. История принадлежит паре пользователь + компания; даже два владельца одной компании по умолчанию не читают чужую переписку.

На обычных экранах справа снизу — спокойная круглая кнопка «Помощник» с подписью для screen reader. Desktop открывает правую панель шириной520px с кнопками «История», «Открыть на весь экран», «Закрыть». На ширине менее768px FAB открывает `/assistant`. Та же panel используется в отдельном route: desktop история слева240px, сообщения справа; mobile история открывается отдельным представлением внутри страницы, composer всегда доступен.

В шапке всегда видны «Помощник настройки», текущая компания и доступность модели. Drawer имеет focus trap, Escape/явную кнопку закрытия и возврат фокуса к FAB; фон inert, пока открыт modal drawer. В полноценной странице работает обычная навигация. `100dvh`, safe-area, min-height/min-width0, textarea и кнопки не уходят под экранную клавиатуру. История и сообщения прокручиваются независимо, горизонтальные scrollbars документа не допускаются.

Закрытие панели только скрывает её, а не обещает отмену серверной работы. Если run ещё идёт, FAB показывает нейтральный индикатор «Помощник выполняет задачу»; повторное открытие восстанавливает состояние через API. При навигации drawer закрывается. При выходе/смене компании/потере прав очищаются вся переписка, черновик, secureform и device-code state, abort старых HTTP/polling/stream запросов; поздние ответы предыдущей компании не применяются.

## Новый диалог и русский текст

Пустое состояние: «Помогу подключить Телфин и настроить рабочее пространство». Подзаголовок: «Проверю текущие настройки, создам нужные статусы, теги и представления». Кнопки-подсказки содержат тот же текст, который отправляется:

- «Помоги подключить Телфин».
- «Создай статусы и теги для отдела продаж».
- «Настрой фильтр пропущенных звонков без перезвона».
- «Проверь, что осталось настроить».

Под composer: «Ключи вводите через “Добавить доступы”, они не попадут в переписку». Основные действия: «Отправить», «Остановить», «Добавить доступы». Enter отправляет, Shift+Enter добавляет строку; IME composition не отправляется. На mobile остаётся явная кнопка. Лимит текста берётся из контракта, пустые сообщения и второй concurrent turn запрещены. Черновик живёт только в памяти текущего user/org; секреты никогда не сохраняются как черновик.

История показывает название, время обновления и нейтральный running/error индикатор; список имеет loading/empty/error/retry, новое сообщение не стирает предыдущие. Создание диалога происходит явно или перед первой отправкой. Переименование — короткая форма; удаление истории требует конкретного подтверждения и не откатывает уже выполненные настройки компании.

## Выполнение и действия инструментов

Показываем только пользовательское состояние и итог, без chain-of-thought/скрытого reasoning. У сообщения отдельная лента действий: «Читаю настройки» → «Проверяю Телфин» → «Создаю тег “…”» → «Сохраняю представление». Closed lookup переводит tool names в русский; неизвестный slug отображается обычным текстом без HTML.

Каждое действие имеет ID, состояние queued/running/completed/failed/uncertain, короткий безопасный результат и ссылку на изменённый ресурс при наличии. Chips не означают success до фактического результата инструмента. Результаты сохраняются в истории и доступны после reload. Ошибка шага остаётся видимой даже при последующем текстовом ответе. После создания/обновления показываем «Открыть настройки»/«Открыть представление»; обычные экраны заново читают API, не полагаются на текст модели.

Состояния run по контракту: queued/running → completed/cancelled/failed/interrupted. UI добавляет локальные idle/submitting/reconnecting, а uncertain относится к конкретному действию tool, не новому terminal state API. Для interrupted текст «Выполнение прервано. Проверьте уже выполненные действия». В случае разрыва сети: «Связь прервалась. Проверяем результат выполнения» и GET актуального run. EOF/step_finish не превращаются автоматически в completed. Повтор отправки использует тот же client_request_id при неизвестном исходе; новый запрос создаётся только после согласования фактического статуса.

«Остановить» сначала отправляет авторизованную отмену конкретного run, затем прекращает локальное ожидание. UI различает подтверждённую отмену, уже завершённый run и неизвестный результат. Текст: «Остановлено. Уже выполненные изменения сохранены». Никакого обещания транзакционного отката настроек/внешних вызовов.

## Защищённые доступы Телфина

«Добавить доступы» открывает inline-card/модальную secureform, визуально отличную от message bubble. Поля: App ID, App Secret (`type=password`), Client ID; дополнительные несекретные параметры доступны по контракту. `autocomplete=off/new-password`, отсутствие query params/localStorage/sessionStorage/аналитики и console logs. Кнопки: «Сохранить доступы» и «Отмена».

Опциональная вкладка «Вставить JSON» расположена только внутри этой формы; пример содержит названия полей, без действительных значений. Строгая локальная проверка object с разрешёнными ключами/строками, без coercion и вывода исходного JSON в ошибку. Не принимаем chat paste как замену secureform. JSON-текст не переносится в composer или сообщение. В обычном composer paste с узнаваемыми ключами app_secret/api_key/token блокируется до передачи и предлагает открыть secureform; серверная проверка остаётся обязательной, поскольку фронтовая эвристика не гарантирует распознавания любого секрета.

Форма отправляет credentials только на специальный авторизованный endpoint. Успех возвращает opaque credential_ref, kind и статус; рядом с composer появляется безопасная карточка «Доступы Телфина добавлены» без app_secret/Client ID/rawJSON; она строится из CredentialSummary и не имитирует пользовательское сообщение. Модель получает только credential_ref. Ни frontend history API, ни tools/result/error не должны возвращать содержимое секрета. Form state очищается после success/close/unmount/смены user/org; ошибка запроса не цитирует body.

Кнопка «Добавить доступы» доступна явно; если ответ помощника просит доступы, пользователь открывает ту же форму. Контракт v1 не содержит waiting_credentials/resume: после сохранения следующий обычный запрос содержит только credential_refs своей conversation. Продолжение незавершённого run не имитируется; форма не отправляет credentials обычным сообщением. Просроченный/чужой credential_ref даёт понятную ошибку и предлагает добавить доступы повторно, без fallback на ручную передачу в чат.

## Доступ модели и Codex

Отдельная карточка «Модель помощника» показывает статусы not_configured/auth_required/authorizing/connected/error/unavailable, полученные только от backend. Она не использует доступность модели анализатора звонков как доказательство авторизации помощника. Наличие runner Docker container также не означает подключённый аккаунт.

Если выбранный runtime действительно поддерживает device flow, кнопка «Войти в аккаунт» получает от backend pending_login.verification_url, user_code, expires_at и auth request ID. UI показывает код и безопасную HTTPS-ссылку, копирование кода, оставшееся время и «Проверить вход»/короткий polling. Device code живёт только в памяти формы и не попадает в chat history/model context/logs. Просроченный код заменяется только новой попыткой. Success только после фактического verified backend state; вкладка/таймер не симулируют авторизацию.

Альтернатива — API key через отдельную форму авторизации Codex с POST `/assistant/auth/start` `{type:"apiKey",api_key}`. Он не передаётся через chat credentials endpoint и не становится credential_ref. Авторизация модели принадлежит компании, а история и Телфин credential handles — конкретному user+org+conversation. Для недоступного device flow пишем «Этот способ входа недоступен в текущем runtime» и показываем поддерживаемый вариант; не генерируем демонстрационные URI/коды. Пока нет реального auth, текст «Подключите модель, чтобы отправить запрос» и рабочая форма настройки; demo обозначается явно и не называется подключённым Codex.

## Предложенное разбиение frontend

- `entities/assistant/model.ts`: server history, active conversation/run, durable events/status, generation-scoped cancellation/reconcile; без секретов.
- `shared/api/assistant.ts`, `shared/interfaces/assistant.ts`: typed REST/polling contract и runtime event validation, включая unknown events и stale run/tenant.
- `features/assistant/AssistantFab.vue`, `AssistantPanel.vue`, `ConversationList.vue`, `MessageList.vue`, `ToolActivity.vue`, `AssistantComposer.vue`.
- `features/assistant/SecureCredentialsForm.vue`, `ModelAccessCard.vue`: Телфин и Codex APIkey идут разными endpoint; секреты только локально внутри соответствующего компонента; не помещать в Pinia model/history.
- `pages/assistant/AssistantPage.vue`, изменение App/router/nav/style после plan-check PASS.

Новый ассистент использует только узкие server tools настройки с проверкой tenant/owner, серверной валидацией и audit. Не расширяет права worker анализа звонков и не даёт ему shell/web/MCP/edit/repository/DB/Docker socket.

## Согласование с контрактом v1

Публичный transport — REST polling1s для активного run (`GET /assistant/runs/:id`), без frontend SSE. Снимки Run содержат durable events и текущие Message.text. Сообщения объединяются по ID с историей диалога, а не заменяют её целиком; events сопоставляются по seq. Partial text — настоящее поле backend; печатание не симулируется. Новый polling начинается только после завершения предыдущего, прекращается на terminal/abort/unmount/смене user/org; background-tab может снижать частоту с immediate reconcile при возвращении.

`client_request_id` — crypto.randomUUID для создания conversation и отправки сообщения; при неизвестном исходе повторяется тот же ключ и тело. Rename передаёт optimistic version. Списки conversations используют limit/cursor, detail/messages bounded согласно серверу. Одновременно только один active run/conversation. Максимум текста8000, refs8, Telphin handles TTL24h; карточка показывает expiry и удаление только временного handle, не отключение подключения.

`GET /assistant/status` даёт available/runner_ready/authenticated/auth_mode/codex_version/model/error_code/pending_login. `assistant_auth_required`409 сохраняет ввод задачи в локальном composer, открывает доступную настройку модели и не добавляет несуществующее message в history. После реального authenticated status можно повторить запрос. Authstart/cancel/logout имеют свои endpoints и не идут в модель. DeviceURL открывается как HTTPS с noopener/noreferrer, без кодов в собственном URL приложения.

Tool events хранят только seq/type/tool_name/title/status/summary. Произвольные HTML/URL/аргументы из модели не отображаются. Переходы к «Настройкам», «Тегам»/«Представлениям» строятся по закрытым известным именам tools; отсутствие конкретного resource ID не выдаётся за прямую ссылку на созданный объект.

## План проверок реализации

Unit/component: role visibility; один panel/store для drawer/page; secret JSON validation и очистка формы; payload Телфин endpoint содержит credentials, Codex key идёт только auth/start, message endpoint содержит только текст/ref; readonly history не получает raw secrets; cancellation+late events+org switch; повторные events/run mismatch/оборванный polling; readiness/auth error и device expiry; tool state persistence/reconcile.

Реальный local API E2E: owner FAB → новая история → задания tools → фактические теги/статусы/сохранённый фильтр; rename/delete; mobile390 без overflow/с доступной кнопкой закрытия; secureform синтетический sentinel не появляется в DOM истории/GET messages/audit; организацияB не видит историю/ref/runA; manager/reviewer denied; stop/disconnect/reload восстанавливают фактический исход. Внешняя авторизация Codex/Телфин проверяется отдельно при доступах, отсутствие не подменяется demo success.
