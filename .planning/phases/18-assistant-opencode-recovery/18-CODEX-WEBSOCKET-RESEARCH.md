# Phase18 — проверка WebSocket proxy закреплённого Codex

Проверен **официальный исходный код tag `rust-v0.167.2`**, Git tree SHA `ce21072c90d32c40af7a55b60b2fb35c84a43a6a`. Это версия CLI текущего assistant image, а не предположение по последнему main. Код приложения и runtime не менялись; user auth/model requests не выполнялись.

## Вывод

**Responses WebSocket использует HTTPS_PROXY и NO_PROXY. Отключать WebSocket или добавлять неизвестный feature flag не требуется.**

- Настоящий [ResponsesWebsocketClient](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/codex-api/src/endpoint/responses_websocket.rs) создаёт WebSocketConnector с тем же HttpClientFactory; эта ветка относится непосредственно к модельному Responses transport.
- В [pinned WebSocket dialer](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/websocket-client/src/dialer.rs) default transport использует proxy-enabled Tungstenite: environment proxy применяется до открытия socket. Ветка explicit proxy с NO_PROXY также обращается к proxy-aware dialer. HTTP CONNECT переносит TLS/WebSocket без расшифровки.
- [Pinned outbound proxy resolver](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/http-client/src/outbound_proxy.rs) для `wss` выбирает HTTPS_PROXY, затем HTTP_PROXY/ALL_PROXY и сохраняет NO_PROXY. На Linux отсутствующая platform proxy discovery переходит к environment; direct route не выбирается вопреки настроенному proxy.
- В [upstream tests этой версии](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/websocket-client/src/dialer_tests.rs) есть отдельные subprocess/no-proxy и secure-WebSocket-through-CONNECT сценарии. **Здесь они изучены, но не запускались**; наши собственные TCP/CLI проверки описаны в основном VERIFICATION.

## Три разрешённых host

Обычный публичный ChatGPT model endpoint в [pinned provider definitions](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/model-provider-info/src/lib.rs) — `chatgpt.com/backend-api/codex`; API-key endpoint — `api.openai.com/v1`. [Device-code implementation](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/login/src/device_code_auth.rs) использует issuer `auth.openai.com` для challenge/poll/token exchange. Эти три host покрывают текущую фиксированную конфигурацию текстового помощника. [OpenAI gateway documentation](https://learn.chatgpt.com/docs/enterprise/connect-to-a-gateway?gateway-auth=oauth) отдельно подтверждает ChatGPT `/models` и `/responses` на том же backend.

Это не обещание поддержки произвольного enterprise gateway, FedRAMP/residency endpoint, voice/plugin/стороннего провайдера или будущей версии CLI. Внешние features в нашем assistant отключены; новая цель будет отвергнута allowlist. Успешный isolated device challenge доказывает OAuth transport; реальный authenticated Responses/WebSocket диалог остаётся пользовательским UAT после личного входа. Ни запрос к модели, ни использование чужой авторизации ради этой проверки не выполнялись.
