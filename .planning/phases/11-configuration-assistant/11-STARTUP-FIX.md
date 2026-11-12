# Исправление authenticated startup — 2026-11-12

Пользователь подтвердил вход в сервис и Codex, но любой prompt завершался ошибкой до ответа. Это фактический дефект user UAT; прежние offline/noauth проверки его не покрыли.

## Research и причина

В закреплённом Codex0.167.2 с ChatGPT auth во время config/read приходит `remoteControl/status/changed`. Это информационное notification с состоянием подключения. Строгая allowlist TurnService его не знала, отклоняла и завершала process. Внешние детали diagnostics терялись: следующее закрытие процесса скрывало первую причину, API сводил runner codes к общему runtime failure. Два подключения показывались дважды, а обычные status при failure превращались в uncertain, хотя ни одного tool action не было.

## Plan и execute

1. Добавить только известное informational notification в IGNORE; запреты tool items, host RPC и остальных неизвестных events сохранить.
2. Сохранять первую причину ошибки при cleanup, писать только safe code/stage/run_id в diagnostics, передавать фиксированные safe error classes в API/UI.
3. Дедуплицировать подряд одинаковые status, при failure закрывать их как failed; uncertain оставить для неподтверждённых tool actions.
4. Дополнить native probe полным HTTP createRunner→TurnService с production thread/turn parameters; отдельно synthetic authenticated-startup notifications. Повторить настоящий prompt в подключённом аккаунте без изменения настроек.

Независимые исполнители написали и проверили regression tests, не меняя root core: runner tests — domain_persistence, API tests — fix_cli_review. Модель наследуется. Новых незакрытых findings они не сообщили.

## Проверки

- Node19/19 PASS,0skips,5.67s. Четыре новых distinct tests: startup informational completed; startup+forbidden deny; startup+unknown deny; настоящий pinned Codex/SDK через полный production orchestrator. Последний использует только локальный Responses fixture и synthetic auth.
- Affected API2/2 PASS на отдельной PostgreSQL: новый startup/dedup/9wire error codes/secret diagnostics и существующий durable partial/malformed regression. Семь distinct acceptance tests в исходниках; весь набор7 заново не заявляется.
- Frontend43/43 PASS, production Docker build включает typecheck.
- Настоящий HTTP запрос через подключённый пользователем ChatGPT account завершился completed, `gpt-6.1-sol` ответил «готово». Проверочный чат удалён через API, auth volume и настройки компании сохранены. Это первый live model smoke; не доказательство качества настройки или Телфина.

Вторая причина найдена при live чтении настроек: известная модель поддерживает tool search и получает MCP deferred; при выключенном code-mode доступные8tools не доходили до реального model-visible каталога. В конфигурации единственного MCP server теперь `omit_tools_from = ["deferred", "code_mode"]`, то есть эти8tools доступны напрямую. Ни OS/code-mode, ни внешние servers не включались. Политика проверена по [исходнику закреплённого Codex0.167.2](https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/core/src/tools/spec_plan.rs) и его config schema.

Финальный Docker19/19 PASS,4.35s. Actual API→Node→ChatGPT `gpt-6.1-sol`→`get_workspace_configuration` PASS: tool eventcompleted, модель прочитала настройки/revision0 и подтвердила отсутствие изменений. Контрольный чат удалён, авторизация сохранена; повторный вход не нужен. Документируются только чтение и простой ответ, реальные mutations/Телфин ещё не проверены.

Pinned model metadata также имеет приоритет над выключенными feature flags: code_mode_only, apply_patch_tool_type и experimental_supported_tools могут вернуть native инструменты. Добавлен readonly public catalog ровно из0.167.2 с license/hash/provenance: сохранены все12model IDs/параметры, изменены только tool_mode=direct, apply_patch_tool_type=null, experimental_supported_tools=[]. tools.experimental_request_user_input.enabled=false убирает UI callbacks. Смена модели/аккаунта или чтение host auth не требуется. Сравнение с исходным public JSON подтвердило сохранение всех остальных metadata.

Actual known-model profile теперь проверяется через productioncatalog и productionorchestrator с локальным Responses backend (включая additional_tools wire). Строгая проверка не допускает exec/apply_patch/OS: forced exec_command, generic JS exec и apply_patch получают unsupported call, marker отсутствует. Это закрывает пробел прежнего unknown-model fallback probe. Финальный Node20/20 PASS локально6.28s и в Docker5.05s,0skips. Изменённые API2/2 и frontend43/43 PASS. API/client/assistant healthy, авторизация пользователя сохранена. Полнота истории Телфина и смысловое качество модели остаются отдельной приёмкой. Временные диагностические scripts/reports лежат только в ignored private data/eval; raw auth.json и секреты не выводились.
