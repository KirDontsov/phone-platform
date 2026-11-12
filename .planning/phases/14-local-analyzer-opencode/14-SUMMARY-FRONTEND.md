# Фаза14 — frontend ASR settings

Дата: 2026-11-12. PLAN-CHECK PASS0/3 прочитан перед execute. Фаза12 остаётся приостановлена. Production source: SettingsPage.vue, shared/lib/processing.ts. Новый тест: tests/asr-opencode-settings.test.ts.

## Реализовано

- В «Настройки → Расшифровка» доступен «OpenCode (Docker) → OpenRouter».
- Два явных варианта модели: `moondream/parakeet-ultra`, `nvidia/nemotron-3.5-transcribe`. ASR dropdown передаёт exact ID, не chat alias и не дополнительный provider prefix.
- При явном выборе нового провайдера форма задаёт адрес `http://model-runner:4096` и модель Parakeet Ultra по умолчанию. Адрес readonly; существующий заданный connector URL не заменяется при загрузке. Сохранённый нестандартный/неизвестный model ID не преобразуется автоматически в другую модель и требует выбора поддерживаемого варианта.
- Поле секрета подписано «API-ключ OpenRouter»: это tenant key. Transport runner password не вводится и не выводится в браузере. Сохранённый API key загружается пустым; unchanged provider + blank сохраняет ключ через существующий server merge. При явном переключении ASR провайдера очищается набранный draft key; попытка сохранить новый реальный provider без нового ключа блокируется. Direct HTTP guard реализует root отдельно.
- Никаких API write при загрузке страницы или автоматической смены настроек компании. Сохранение выполняется только явным submit. Reviewer fields disabled, mutation controls отсутствуют, save handler дополнительно отклоняет readonly/double submit.
- Legacy «Нейросетевой анализ → OpenCode runner» оставлен со свободным полем text model и прежним API-key label/семантикой; OpenAI/http/demo варианты ASR сохраняются.
- Объяснение в UI: Docker connector использует ASR adapter; туннель и компьютер должны работать, OpenRouter оплачивается отдельно, вход в Codex не подключает этот API. Demo warning следует выбранному provider draft, а не предыдущему сохранённому значению.
- Добавлены безопасные пользовательские сообщения `unsupported_asr_model`, `asr_runner_url_mismatch`, `asr_runner_auth_missing`, `asr_provider_key_required`.

## Проверки

- Новые3 component regression tests сначала RED3/3 на предыдущем интерфейсе: отсутствующий OpenCode ASR/модели, перенос blank старого секрета при смене provider, отсутствующий readonly model select. Затем GREEN3/3.
- Контракт сохранения проверен с обеими exact моделями: Nvidia с новым synthetic ключом, Parakeet с blank unchanged secret. Дополнительные assertions: draft старого ключа очищается; после save введённый ключ удаляется из формы, GET не возвращает plaintext; test endpoint `/connections/asr/test`; legacy analysis свободная модель; reviewer без mutations, включая искусственный form submit.
- Полный frontend75/75 PASS (17 test files), vue-tsc/Vite build PASS. После этих прогонов изменена только condition demo warning с сохранённого provider на выбранный draft; финальный Docker build/browser UAT фиксирует root.
- Для трёх новых tests RED подтверждён; дополнительные guards внутри них отдельно не мутировались. Provider/model/телефония/удалённый server и платные ASR запросы не вызывались.

Приватные логи вне Git: data/eval/phase14-asr-ui-red.log, phase14-asr-ui-green.log, phase14-frontend-tests.log, phase14-frontend-build.log. `git diff --check` PASS.

## Селекторы для browser UAT

Открыть `/settings/asr`. `getByLabel('Провайдер',{exact:true})` → `selectOption('opencode')`; `getByLabel('Модель',{exact:true})` → exact model ID; `getByLabel('Адрес API / сервера',{exact:true})` readonly; `getByLabel('API-ключ OpenRouter',{exact:true})` password input. «Сохранить», «Проверить соединение». Никаких записей в настоящую компанию во время fixture UAT.
