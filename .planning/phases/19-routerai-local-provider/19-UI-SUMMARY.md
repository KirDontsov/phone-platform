# Phase19 — RouterAI в настройках OpenCode

Автор: routerai_ui. Gate:19-PLAN-CHECK PASS revision1 проверен до изменений.

Изменены только SettingsPage.vue и tests/opencode-local-settings.test.ts. Сохранение уже использовало provider из выбранного catalogentry в исходномHEAD; ложное предположение о hardcodedapinex не превращалось в ненужную переработку. Добавлен billinghint по реально выбранной локальной ASRмодели: RouterAI использует свой аккаунт, APInexVoiceSTT — свой, до выбора подпись нейтральная. Сохранение по-прежнему только по явной команде, без APIkey в localmode и без автозапуска обработки.

Три новые регрессии проверяют обе исходные RouterAIмодели moondream/parakeet-ultra/nvidia/nemotron-3.5-transcribe: до выбора пустое значение и0записей; после выбора RouterAIhint, APIkeyfield отсутствует; explicit save передаёт local_provider:routerai и credential_source:local, без api_key. Те же modelIDs в отдельном legacyOpenRoutermode требуют свойключ, не сохраняют local_provider и не дают автоматических записей. Третий сценарий проверяет нейтральную подпись до выбора и корректныйAPInexhint при переключении.

RED доfix:3новыхFAIL/5старыхPASS (phase19-ui-red.log). GREEN:8/8 targeted,98/98 полный frontend, typecheck/Vitebuild PASS. gitdiffcheck PASS. Приватные evidence data/eval/phase19-{ui-red,ui-green,frontend-green,frontend-build}.log. Реальные провайдеры, credentials и production этим агентом не вызывались/не менялись. Публикация/realbrowser относятся кroot verifier/UAT.
