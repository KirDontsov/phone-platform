# Phase20 independent verification

Дата: 2026-11-13. **Текущий статус: BLOCKED — production retention incident; revision1 implementation/recovery pending.** Это implementation verifier, не PLAN-CHECK. Первичный code5f633b8306173b76d798a999e5781a345400e22e опубликован, но выпуск не принят.

## Подтверждённые локальные результаты

- Независимые reviews20-ROOT-REVIEW.md/20-WORKER-REVIEW.md: первичные замечания callback expiry и UI ожидания закрыты, исходный narrow scope PASS.
- Независимый current source run: Python14/14, scoped recording UI5/5, actual isolated PostgreSQL persistence4/4, workerlib20/pipeline8/providers16/modelHTTP13 —61 Rust checks PASS без skips. Shared Cargo target RED-copy incident устранён очисткой own package artifacts и независимым current GREEN.
- Операторские логи: полныйfrontend98/type/build PASS, importer8/analyzer11 PASS. OAuth actual localhost fixture Chrome: metaRefresh действительно очищает query, safe exchange error виден, mobile390 overflow0/JS0; реальных RouterAI вызовов не было. Новые OAuth3 и lifecycle1 RED логи просмотрены verifier; worker RED4 описан исполнителем, не повторён независимым reviewer.
- Пользовательский RouterAI consent не завершён: приватный runtime ранее не содержал ключа. Причина пользовательской пустой страницы на RouterAI неизвестна; callback UX исправлен, реальный account/model/ASR не объявлены проверенными.

## Первый release: что проверено независимо

После root activation сигнала выполнены только readonly SSH/SQL/metadata. Восемь собственных production сервисов healthy. API/worker/client имеют15-5f633b8 и совпадают с native linux/amd64 build IDs. Assistant/relay оставлены живыми под15-d59a82e; их actual IDs совпадают с соответствующими15-5f633b8 images, ненужных restart не было. Activation log подтверждает worker_active_leases=0 до замены.

Оригинальный sync job сохранён:43370 calls,attempt2,fencing4,window33/page7,coverage_complete=false. В09:01 UTC cooldown уже истёк; поэтому production proof «attempts не расходуются во время cooldown» этим наблюдением **не получен** (локальные actualPG checks PASS).

## Блокирующее production finding

Независимые snapshots09:01:09/09:01:23 UTC показали снижение recording total33930→33926 и ready5193→5189, без движения original sync. Причина установлена кодом/SQL: существующий retention90days использует started_at, после нового приоритета запустил удаление недавно импортированных исторических звонков. Неявное включение retention конфликтует с полным импортом. File metadata check во время purge проверил5188 путей,2 уже отсутствовали; это concurrent deletion race, а не доказательство сохранности всех файлов.

Root немедленно paused единственный собственный worker. Readonly09:04:19 UTC:24648 exact retention-origin targets,все импортированы менее2дней назад;120 уже purged,24528 softdeleted и ещё целы; manualcall.deleted audits0/manualpurge jobs0. У целых softdeleted сохраняются2804 ready и16141 pending recording children. Приоритет purge также оставил sync за24528queued purge jobs.

Сохранён private0600 `data/eval/phase20-retention-targets.json`: exact org/retention-job/dedup proof, call/job IDs, provider IDs всех120purged, ручные исключения и counts; никаких идентификаторов/телефонов/секретов вGit или обычных логах. Источники восстановления пока не вызывались. Единственный найденный local/remote production-20261112 backup имеетbaseline759calls/614recordings и предшествует affected company import; он не доказывает возможность вернуть эти120 новых звонков/ручные поля.

Дополнительная readonly проверка120purged: nonpurge audit events отсутствуют, завершённых analyze/transcribe/reprocess jobs не найдено,10transcribe jobs ещё queued. Нет свидетельств потери готовых расшифровок/ручной разметки, но метаданные и дочерние записи удалены фактически. После реального source recovery проверить согласованность оставшихся processing references; отсутствие audit не заменяет backup.

## Gate revision1

20-PLAN-CHECK.md revision1 PASS разрешает выполнение explicit opt-in/live purge guard и guarded bulk recovery; финальный release, visible-history recovery, real provider source repair120 и human UAT остаются **не проверены / pending**. Worker должен оставаться paused до безопасной реализации/recovery. Не считать фазу принятой по прежним тестам, не скрывать уже произошедшее удаление и не фабриковать восстановленные данные.
