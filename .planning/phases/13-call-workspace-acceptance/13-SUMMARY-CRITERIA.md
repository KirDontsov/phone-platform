# Criteria / integration execute evidence

План прошёл independent PLAN-CHECK PASS revision0/3 перед execute. Root блок: evaluation_mode score/checklist в rubric API/MCP/TS, explicit fulfilled true/false/null и строгие evidence/type/score guards в domain, unknown roles suppress verdicts, historical score mode не превращается в Да/Нет. Worker prompt и synthetic provider поддерживают checklist. Новая версия prompt привязана pipeline agent. Старые данные не переписывались.

Settings: примеры3 критериев, инструкции/режим/immutable active version. Sheets поля→колонки вместоJSON, stable keyA, frontend иAPI validation duplicates/range/unknown, итог/договорённости/следующиешаги/criteriaJSON export. ASR demo warning и пояснение про отдельный provider от Codex chat. Integrations теперь показывает ASR, demo отмечен явно. Сценарии docs/customer-workflows.md; Google IMPORTRANGE/VLOOKUP сверены с официальной справкой, live Google не вызывался. Одна выбранная таблица/лист компании явно указана.

Проверки:

- Domain17/17 PASS на момент запуска (включая3 новых checklist; все3 RED на предыдущей реализации).
- Worker providers16/16 PASS; новое Sheets fields/verdict test RED → GREEN. Demo checklist test GREEN, его RED отдельно не проверялся.
- Settings UI2/2 PASS; оба RED на прежнем интерфейсе. В ходе теста исправлены fixture assertions (input.value и submit event), затем проверены позитивное сохранение и запрет duplicate columns.
- API rubric unit1PASS (legacy+valid modes+invalid types); RED отдельно не проверялся.
- Frontend typecheck PASS; общая сборка/независимый verifier/браузер будут зафиксированы отдельным итогом фазы.

Реальные ASR/LLM/CRM/Google запросы не запускались. Подключения компании и текущие критерии автоматически не менялись. Качество/живые доступы pending.
