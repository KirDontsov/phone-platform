# Независимая проверка реализации — фаза13

Дата: 2026-11-12. Проверяющий: phase13_checker, отдельный агент; production source не редактировал. Проверены исходники, контракты, отчёты исполнителей, фактические логи GREEN/RED и Docker browser screenshots. PLAN-CHECK не использовался как evidence готовности.

**Локальная реализация: PASS. Внешняя и финальная пользовательская приёмка: PENDING.** Установленный demo ASR не обрабатывает реальные записи; фаза не объявляется принятым production ASR/AI решением.

## Соответствие требованиям

| Сценарий | Проверенный результат | Ограничение |
|---|---|---|
| Пропущенные без связи X часов | Относительный numeric AST, только первый звонок эпизода, open/overdue отдельно от uncertain; age/attempts до date/search; successful follow-up вне периода закрывает эпизод | Неполная история не считается доказанным отсутствием перезвона |
| Произвольные требования | Версионированный rubric с инструкцией, score/checklist, явный fulfilled true/false/null, evidence и trusted mode; unknown roles подавляют verdict | Качество реального ASR/модели по customer corpus не проверено |
| Теги и этапы | Тип обращения отдельно от бизнес-статуса/тегов/технического исхода; Dashboard/журнал показывают разметку, правая панель сохраняет CAS/manual lock/draft | Ручные изменения требуют сохранения; reviewer read-only |
| Сделки/Таблицы | amoCRM связь/экспорт сохранены; Sheets mapping полей, summary/agreements/next_steps/criteria и RAW stable key A; current transcript analysis + score/null | Одна таблица/лист на компанию, live OAuth/доставка не проверены |
| Понятная обработка | Текст/анализ определяются документами, partial≠missing; queued/running/retry/failed, safe причины, собственная очередь, next retry; preflight и active dedup | ETA неизвестен; configured readiness не равен live-проверке |
| Рабочий интерфейс | Полная ширина, фильтры на Dashboard/журнале, одинаковые applied filter/date для KPI/drilldown, sidebar и mobile | Длинная таблица использует локальную прокрутку; финальный human UAT остаётся пользователю |

## Найденные и закрытые замечания

- Dashboard no-field переход терял фильтр; unhandled переход терял дату/условия; analyzed+partial счётчик раскрывался только в analyzed. Теперь drilldown берёт успешно применённый snapshot и одинаковые условия.
- Неверный интервал мог теряться при комбинировании фильтров. Теперь blank/negative значения блокируют запрос, ошибка очищается при отключении условия.
- Sidebar мог потерять исходный ID при dirty prop change, первый Shift+Tab выходил из панели. Editing ID и focus guard защищены; polling/409 сохраняют draft и исходную версию.
- Старый успешный child скрывал ошибку нового wrapper. Приоритет terminal failed wrapper исправлен; предыдущий partial результат сохраняется отдельно.
- Экспорт выбирал прежний анализ после новой расшифровки. Новый export выбирает analysis последнего transcript и score из него; existing delivery сохраняет pinned ID/null. Недоступный pinned результат не подменяется.
- Уточнены static error codes и direct HTTP validation instruction types; API/TS/AST/MCP rubric modes согласованы.

Открытых блокеров реализации в проверенном scope не обнаружено. Это не гарантия отсутствия дефектов вне проверенных сценариев.

## Проверки и доказательства

- PostgreSQL/Actix: processing_contract1 и recording_availability1 PASS на отдельных тестовых данных. Проверены concurrent200/202 с одним job, preflight422/no-enqueue, >200 исторических jobs, foreign tenant/same UUID, safe redaction, active/terminal priority, current analysis/partial, callback parity calls/dashboard/callbacks и follow-up вне периода.
- Worker PostgreSQL pipeline7/7 PASS: current/obsolete analysis и score, pinned old/null/missing; прежние manual overrides/versions/budget/purge/uncertain delivery regressions сохранены.
- Финальный consolidated domain18/18 PASS (phase13-domain-final.log), включая checklist и относительные callback/filter guards. Worker providers16/16 PASS, API rubric unit1 PASS.
- Финальный consolidated frontend72/72 PASS (phase13-frontend-final.log); typecheck/Vite build PASS. После mobile/sticky размещения затронутые workspace/recording tests10/10 PASS, финальный Docker client build PASS.
- Docker API/worker/client пересобраны и healthy (phase13-up.log). Browser UAT с синтетическим API fixture поверх Docker/nginx:3/3 PASS (1920/390, sidebar/filters, queue/missingASR/checklist). Дополнительный root Vite fixture smoke также PASS, но не считается отдельной внешней интеграцией.
- Root дополнительно выполнил read-only browser smoke против настоящего Docker API:699 звонков всего,100 строк с processing/transcript/analysis summary; для реального звонка виден demo ASR blocker, кнопка расшифровки disabled, JS errors0, external provider requests0. Проверочная сессия завершена logout; настройки и данные не записывались. Это проверка настоящего состояния приложения, без реального ASR/LLM inference.
- Проверяющий лично посмотрел workspace-1920.png и workspace-390.png из browser UAT: drawer читаем, мобильная панель помещается в экран; screenshots содержат только синтетические данные.
- git diff --check PASS.

RED evidence:3 новых checklist domain regression, Sheets fields/verdict regression и2 settings UI regression;4 исходных frontend workspace regression; дополнительный sidebar dirty/CAS и Dashboard snapshot regression. Backend основной projection regression RED на прежнем поведении и4 отдельные mutation RED по assertion: preflight, concurrent dedup, failed-wrapper priority, export currentness. Для остальных дополнительных guard cases RED отдельно не проверялся; demo checklist unit и rubric unit имеют GREEN без отдельного RED.

Логи и screenshots приватны, вне Git: data/eval/phase13-*.log, phase13-backend-red-summary.json; phone-client/test-results/customer-workspace-*/workspace-{1920,390}.png. API/worker отдельные тестовые БД удалены. Точные команды и результаты исполнителей:13-SUMMARY-BACKEND.md,13-SUMMARY-FRONTEND.md,13-SUMMARY-CRITERIA.md.

## Внешние gates

Рабочий ASR и отдельный analysis provider должны быть настроены владельцем; авторизация Codex помощника не даёт доступ к распознаванию аудио. Реальное качество транскрипции/ролей/оценки по выбранной rubric, live amoCRM/Google OAuth и доставка остаются PENDING. Платные реальные model/ASR запросы в этой фазе не запускались. Пользовательские подключения, записи, критерии и авторизация автоматически не менялись. Фаза12 остаётся приостановлена, stash не применялся.

Финальный пользовательский UAT описывается отдельно в13-UAT.md; acceptance фаз1–12 этим отчётом не закрывается.
