# Phase18 — OpenCode settings UI

Реализация: opencode_settings_ui; план18-01/PLAN-CHECK PASSrevision1 прочитан до application edits.

- Анализ через OpenCode использует фиксированный readonly адрес локального коннектора, выбор модели из sanitized `/connections/local-models`; поле внутреннего pairing/APIkey не показывается и не отправляется.
- Расшифровка OpenCode разделяет `credential_source=local` и явный OpenRouter `api_key`. Local APInex Voice STT выбирается вручную из своего каталога, без default model/auto-save/auto-processing. Сохраняется `local_provider` выбранной модели; APIkey в local payload отсутствует. Существующие OpenRouter конфиги без credential_source остаются в legacy key-mode с исходными Parakeet/Nemotron IDs.
- Указано, что local ASR использует существующий лимит/баланс APInex. Он не называется Parakeet/Nemotron и не выдаётся за возможность подписки Codex. Отсутствие каталога/сбой туннеля показываются отдельным понятным сообщением; сохранить несуществующую локальную модель нельзя. Reviewer не меняет настройки.
- Загрузка каталога не сохраняет настройки, не вызывает модель/ASR, отсутствие endpoint не ломает остальные разделы.

Пять новых component tests сначала проверены RED на исходном UI:5/5 failed, `data/eval/phase18-ui-red.log`. После исправления:5/5 GREEN. Обновлены три старых ожидания, относящиеся к явно изменённому UI: legacy OpenRouter теперь выбирается отдельно, model analysis — select вместо text input, внутренний APIkey отсутствует.

Полный frontend:95/95 PASS,20testfiles, `data/eval/phase18-ui-full.log`. Vue typecheck/Vite build PASS в `data/eval/phase18-ui-build.log`. Деплой, реальный browser на новом production UI, OAuth и upstream inference этим component suite не подтверждаются и выполняются root/verifier отдельно.
