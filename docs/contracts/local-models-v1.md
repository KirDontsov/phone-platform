# Локальные модели v1

`GET /api/connections/local-models` требует проверенной серверной сессии и роли owner. Возвращает обычный envelope `data`:

```json
{"available":true,"providers":[{"id":"provider","name":"Existing provider","models":[{"id":"provider/model","name":"Existing model"}]}],"asr_models":[]}
```

При отсутствии доступного этой компании коннектора: `available:false`, пустые массивы. Provider IDs и model IDs получаются из отдельного runtime-конфига коннектора, а не из frontend предположений. Ключи, заголовки и URL провайдера не возвращаются. Это каталог настроек, не доказательство успешного inference.

Для локальной расшифровки config содержит `provider:opencode`, `credential_source:local`, `local_provider:apinex`, `model:apinex/voice-stt`, `base_url:http://model-runner:4096`, без `api_key`. Это явно выбранный APInex Voice STT. Для прежних двух моделей OpenRouter сохраняется `credential_source:api_key` и собственный ключ компании. Автоматического перехода между ними нет.

Для анализа `provider:opencode` использует полный model ID из каталога. Внутренний pairing задаётся сервером и не переносится при смене провайдера/адреса; новый внешний адрес требует нового внешнего ключа. Backend канонизирует локальный URL и сам задаёт `_organization_id` из сессии; worker повторно задаёт его из job. Значение из пользовательского config не определяет компанию.

Коннектор проверяет внутренний `X-Phone-Organization`, привязку своего runtime, владельца каждой native session и модель из разрешённого каталога. Все native config/provider/session маршруты закрыты для неизвестной компании; credentials редактируются из ответов. Публичные для внутреннего healthprobe маршруты `/global/health` и `/doc` не содержат credentials. Текстовый test и ASR test проверяют доступ без inference/аудио. APInex тарификация STT не выдаётся за долларовый cost или LLM token usage.
