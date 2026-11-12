# Phase18 — независимая проверка плана

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker; автор плана — root.

**Статус: PASS, revision1/3.** Автор согласовал следующие конкретизации, они внесены в must_haves и verify соответствующих задач. Изначальная структура valid=true,3tasks,errors/warnings=[]; область задачи и подтверждённые причины корректны.

1. **Сохранение auth при failed reauthorization.** Уточнить, что сохраняются согласованно credential file, runner authenticated state и API confirmed_generation; одного отказа от removeAuth недостаточно, если старый generation станет неисполняемым. Добавить regression failed reauth→старый confirmed login остаётся usable; explicit logout/revoke→и старый login, и pending attempt не usable. Новая попытка не должна обходить session/generation guards или возвращать access в другой tenant.
2. **Изоляция глобально импортированных provider keys.** Уточнить, что tenant guard закрывает все существующие native execution/session/config/provider proxy paths, а не только новый catalog endpoint. Session связывается с org; missing/wrong org не выполняет запрос и не читает чужую session. Generic /config и /provider не раскрывают imported apiKey и не обходят sanitized catalog. Добавить реальные negative/proxy regressions на эти обходы. CONNECT принимает только exact hosts:443 и публичные DNS addresses, отвергая private/loopback/link-local targets.

Уточнения приняты автором и внесены проверяющим по его поручению; блокирующих замечаний после revision1 нет. Существующий ограниченный SSH transport, proxyenv только явный, MCP direct; никакого обхода tenant credentials, повторного ввода уже доступного ключа, автооплаты/STTпереключения или изменения Telphin retry. Реальный isolated device challenge достаточен для transport gate, но не доказывает завершённый user OAuth/model turn. APInexVoice остаётся условным исследованием до primary protocol check и не выдаётся за Parakeet/Nemotron. Собственные runtime600 files не копируют plugins/MCP/tools/Bondscan.

Release должен сохранить worker lease/checkpoint/backoff; TaskC теперь явно требует безопасно активировать изменённый worker-код при отсутствии active lease либо честно отметить соответствующее поведение недеплоенным. Никакого reset jobs/next_attempt_at. Полный import phase17 и последующий configured/polling остаются external quota pending.

Повторный GSD-tools verify plan-structure после всех уточнений: valid=true,3tasks,errors/warnings=[]. Автор отдельно подтвердил worker rollout: сначала readonly activelease check, если lease есть — bounded wait, без forced state changes.
