# Фаза13: frontend implementation summary

Дата: 2026-11-12. Автор реализации: ux_research. PLAN-CHECK PASS0/3 предшествовал исполнению. Фаза12 не возобновлялась.

## Реализовано

- Dashboard/журнал используют всю ширину рабочего окна. Таблица показывает явный анализ (`Проанализирован`, `Анализ с оговорками`, `Не проанализирован`), готовый текст/активный этап/ошибку, бизнес-статус и все назначенные теги. Оценка null не означает отсутствие анализа. На mobile анализ и запись под клиентом; action column закреплена справа.
- Общий `CallFilters.vue` для Dashboard/Calls/Callbacks: taxonomy dropdown по именам, тип обращения, анализ, относительный интервал без успешной связи в часах, отдельно отсутствие попыток. Confirmed выборка включает open/overdue, uncertain остаётся отдельно. Конструктор получает taxonomy и человеческие enum labels; UUID не требуется вводить вручную.
- Dashboard применяет одинаковый AST к KPI/списку и хранит успешно применённый filter/date snapshot для drilldown. Редактирование ещё не применённого фильтра не меняет переход по старому счётчику. Total, все звонки, обработка и анализ (analyzed+partial) сохраняют scope.
- Правый `CallActionSidebar.vue` открывается из Dashboard/Calls без перехода со списка. Manager сохраняет status/kind/tags/comment/manual_lock с исходным CAS version. Polling не сбрасывает draft;409 сохраняет ввод. Dirty close с явным выбором, Esc/focus trap/возврат фокуса, reviewer read-only, ограниченный call/taxonomy API без owner config. Dirty prop change не меняет editing call id.
- Detail `ProcessingPanel.vue`: реальный этап, retry/error, попытки, следующая попытка, scoped queue counts/ahead и unknown ETA; конкретные safe причины отсутствующей ASR/analysis config/demo-only, ссылки владельцу. Повторный submit блокируется при активной задаче/неготовых prerequisites; reprocess reused учитывается. Нет автоматического платного model/ASR запроса от просмотра UI.
- Актуальный analysis по latest transcript id; старые результаты выбираются явно и помечаются предыдущими. Checklist отображает validated fulfilled Да/Нет/Недостаточно данных/Неприменимо; старый numeric score не преобразуется в boolean.

## Проверки

- Полный frontend:72/72 PASS, включая2 settings теста root. `vue-tsc --noEmit` и Vite production build PASS. После этого добавлены только sticky/mobile размещение badges; affected workspace/recording-table10/10 PASS (без отдельного повторного полного build, текущий Docker build выполняет root).
-14 новых frontend regressions:8 workspace и6 sidebar. Первые4 workspace проверки подтверждены RED на прежних существующих компонентах до реализации: analysis/status/tags/manage, taxonomy choices, relative callback filter, queue/provider rendering. После фиксов все PASS.
- Дополнительный RED: временно отключён dirty guard Sidebar.adopt — polling/CAS regression падает; возвращён guard. Временно возвращён no-field drill filter drop — Dashboard snapshot regression падает; исходники восстановлены. Для остальных новых отдельных тестов RED отдельно не проверялся.
- Sidebar guards:CAS7/manual lock/архивная taxonomy, dirty polling+409, Esc/discard/focus, reviewer nomutations, dirty callId switch, initial Shift+Tab trap.
- Workspace guards: backend projection labels vs score=null, relative2.5h→150min, confirmed callbacks excluding uncertain, blank/negative interval не вызывает запрос, error clear, applied Dashboard filter/date identity, current transcript vs previous analysis, checklist/legacy distinction.
- Все фикстуры синтетические, внешние ASR/LLM/Телфин/Google/CRM запросы не выполнялись. Секреты/номера/аудио не использованы в тестах. Root выполняет browser1920/1440/390 и фактический Docker deploy; эти проверки здесь не объявляются завершёнными.

Артефакты вне Git: `data/eval/phase13-frontend-red.log`, `phase13-sidebar-red.log`, `phase13-dashboard-red.log`, `phase13-frontend-tests.log`, `phase13-frontend-build.log`, `phase13-mobile-actions-tests.log`.
