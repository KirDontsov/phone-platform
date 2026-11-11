# GSD: полный цикл и правила переходов

Дата: 2026-11-11. Применены локальные skills `gsd-new-project`, `gsd-plan-phase` и стандартный `references/revision-loop.md`. Названный пользователем plan-verifier соответствует роли `gsd-plan-checker`; проверка реализации — отдельный `gsd-verifier`.

## Что делаем сейчас

Исследование Bondscan/API/AI → требования → roadmap всех фаз → конкретные PLAN.md → независимая проверка → доработка → повторная проверка. Реализация и live-проверки ещё не запускаются. Полный набор планов поздних фаз — baseline, который обновляется по фактам предыдущей фазы.

У пользователя уже есть явное поручение создать план; дополнительное подтверждение на создание документов не требуется. Продуктовые вопросы вынесены отдельно, предположения помечены в PROJECT.md. Выпуск и доступ к реальным сервисам не считаются состоявшимися из-за завершения планирования.

## Артефакты каждой фазы

| Шаг | Вход | Выход и gate |
|-----|------|--------------|
| Research | Цель, требования, результаты предыдущих фаз, официальные docs | `NN-RESEARCH.md`: known/unknown, sources, решения, риски, способы проверки; live-неизвестные не объявлены resolved |
| Plan | Research + context + dependency evidence | `NN-PP-PLAN.md`: frontmatter, требования, зависимости, файлы, действия, verify, done, must_haves |
| Plan-verifier | Все планы фазы и source requirements | `PLAN-CHECK.md`: BLOCKER/WARNING/INFO с конкретным исправлением, результат pass/issues_found |
| Revision-loop | YAML issues checker | Точечные правки planner + новый независимый checker; история в `REVISION-LOG.md` |
| Execute | Проверенный актуальный план и доступные preconditions | Код, миграции, необходимые тесты, `NN-PP-SUMMARY.md` с реальными командами/результатами и deviations |
| Verifier | Реализация и must_haves | `NN-VERIFICATION.md`: passed / gaps_found / human_needed, ссылки на evidence; заполненные планы не являются evidence |
| Revision реализации | Gap report | Исправление или отдельный gap plan, повтор relevant checks; максимум 3 цикла |
| UAT | Рабочая сборка и сценарии | `NN-UAT.md`: наблюдения пользователя, дефекты/решения; затем обновление STATE/ROADMAP/REQUIREMENTS |

Фактические имена CONTEXT/RESEARCH в каталоге могут быть без NN-префикса; файл явно относится к одной фазе.

## Check → Revise → Check

1. Независимый checker читает исходные требования и планы, трассирует каждое requirement до конкретной задачи и проверки.
2. Проверяет зависимости и waves, wiring API/UI/worker, неизвестные внешние контракты, данные/безопасность и выполнимость объёма.
3. Если только INFO — pass. Любой BLOCKER или WARNING требует доработки либо явно обоснованного снятия reviewer.
4. Planner получает исходные issues verbatim, исправляет их, не сокращая пользовательский scope молча.
5. Новый checker проверяет весь затронутый набор и отсутствие регрессий. Не доверять «исправлено» без чтения файлов.
6. Максимум 3 итерации доработки. Если число BLOCKER+WARNING не уменьшается, остановить цикл раньше, показать оставшиеся проблемы и конкретные варианты решения.
7. Неразрешённые существенные вопросы отмечать как `needs_user_decision`; не писать pass и не переходить к зависимой реализации.

В этой сессии `PLAN-CHECK.md` в корне .planning может содержать единый аудит всех фаз с перечислением каждой фазы. Перед execution дополнительно проверяется изменившаяся фаза.

## Что проверяет plan-verifier

- 100% requirement coverage, в том числе negative/failure cases.
- У каждой auto-задачи есть files/action/verify/done, у каждого плана must_haves и реальные upstream dependencies.
- Не предполагаются live ключи/права/история, не доказанные источниками.
- Нет смешения call/cdr, provider status/business status и attempt/connected.
- Незавершённая история не приводит к ложному primary/no-callback.
- Есть tenant isolation, secrets policy, media access, retention и защита ручной разметки.
- ASR/AI контракт, versions/evidence, timeout/cost/retry и воспроизводимая оценка качества.
- Конструктор ограничен безопасным typed AST, время и late updates учтены.
- CRM/Sheets effect не дублируется при неизвестном исходе запроса и повторе.
- Дашборд агрегирует ту же выборку, а unknown не превращает в score=0.
- Объём плана укладывается в отдельную executor-сессию; слишком большой план делится перед execution с повторной проверкой зависимостей.

## Runtime и команды

GSD установлен под `~/.claude/skills` и `~/.claude/get-shit-done`; отдельной команды `gsd-sdk` в PATH нет. В текущей Codex-сессии роли запускаются доступными агентами с чтением соответствующих GSD role docs; модель наследуется. Не объявлять, что недоступная slash-команда была выполнена.

Проверенные utility entry points:

```bash
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs init new-project
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/<phase>/<plan>-PLAN.md
```

В среде с подключёнными GSD skills следующий рабочий шаг — `gsd-execute-phase 1`; в текущем runtime можно исполнять первый PLAN.md напрямую по этому процессу. Нельзя повторно инициализировать проект поверх этих документов. Внешний cross-AI `gsd-plan-review-convergence` сейчас не запускался: независимый checker использует стандартный внутренний revision-loop.

## Preflight для реализации

Фаза 1: пользователь подтвердил имеющийся доступ к API Телфина и записям. Настроить его локально для конкретной компании, проверить hostname/права/принадлежность АТС и выбрать контрольный период в кабинете. Secrets вне Git; подтверждение пользователя не заменяет фактический probe и сверку. При недоступной локальной конфигурации подготовить CLI и fixtures, live acceptance оставить human_needed; M0 не закрывать.

Фазы 5–6: доступны разрешённые для обработки записи и выбранный ASR/LLM provider, его runtime credentials и согласованный предел расходов. Нет provider access — можно проверить adapter contract, но не ставить production-ready.

Фаза 8: подтверждённые sandbox/account и права amoCRM/Google. Внешние writes сначала тестируются на выделенных объектах; реальная связь/экспорт задаётся владельцем в UI.

Фаза 10: согласованы load profile, rubric acceptance и эксплуатационное окружение. Планирование не подменяет разрешение на публикацию.
